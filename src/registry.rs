use crate::{i18n::I18nStore, types::PackageJson};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    process::Output,
};
use tokio::{fs, io::Result, process::Command};

#[derive(Serialize, Deserialize)]
pub struct Registry {
    version: u8,
    packages: Vec<Package>,
}

#[derive(Serialize, Deserialize)]
pub struct Package {
    name: String,
    version: String,
    title: String,
    description: String,
    author: String,
    require: BTreeMap<String, String>,
    dist: Dist,
}

#[derive(Serialize, Deserialize)]
struct Dist {
    r#type: String,
    url: String,
    shasum: String,
}

pub async fn clone() -> Result<Output> {
    info!("Cloning registry repository into '.dist' directory...");
    Command::new("git")
        .arg("clone")
        .arg("https://github.com/bs-community/plugins-dist.git")
        .arg(".dist")
        .arg("--depth=1")
        .output()
        .await
}

#[allow(dead_code)]
fn to_map(list: Vec<Package>) -> BTreeMap<String, Package> {
    list.into_iter()
        .map(|package| (package.name.clone(), package))
        .collect()
}

#[allow(dead_code)]
fn to_list(map: BTreeMap<String, Package>) -> Vec<Package> {
    map.into_iter().map(|(_, package)| package).collect()
}

async fn read_registry(path: impl AsRef<Path>) -> Result<BTreeMap<String, Package>> {
    let json = fs::read_to_string(path).await?;
    let registry = serde_json::from_str::<Registry>(&json)
        .map_err(|e| {
            error!("Failed to parse previous registry data.");
            e
        })
        .expect("Failed to parse previous registry data.");

    Ok(to_map(registry.packages))
}

async fn update_registry<'a, S1, S2>(
    packages: &'a mut BTreeMap<String, Package>,
    plugins_dir: S1,
    updated: impl Iterator<Item = (S2, S2)>,
    hashes: &'a HashMap<&'a str, String>,
    lang: &'static str,
    i18n_store: &'a I18nStore,
) -> Result<()>
where
    S1: AsRef<str>,
    S2: AsRef<str>,
{
    for (name, version) in updated {
        let name = name.as_ref();
        let version = version.as_ref();
        let json = fs::read_to_string(format!(
            "{}/plugins/{}/package.json",
            plugins_dir.as_ref(),
            name
        ))
        .await?;
        let package_json = serde_json::from_str::<PackageJson>(&json)
            .map_err(|e| {
                error!("Failed to parse 'package.json` of plugin '{}'.", name);
                e
            })
            .expect("Failed to parse 'package.json' file.");

        let i18n = i18n_store
            .get(&name)
            .or_else(|| {
                error!("Cannot retrieve i18n texts of plugin {}.", name);
                None
            })
            .expect("Cannot retrieve i18n texts.");

        packages.insert(
            name.to_owned(),
            Package {
                name: package_json.name,
                version: package_json.version,
                title: match lang {
                    "en" => i18n.title.en.clone(),
                    "zh_CN" => i18n.title.zh_cn.clone(),
                    _ => package_json.title,
                },
                description: match lang {
                    "en" => i18n.description.en.clone(),
                    "zh_CN" => i18n.description.zh_cn.clone(),
                    _ => package_json.description,
                },
                author: package_json.author,
                require: package_json.require,
                dist: Dist {
                    r#type: String::from("zip"),
                    url: format!(
                        "https://cdn.jsdelivr.net/gh/bs-community/plugins-dist/{}_{}.zip",
                        name, version
                    ),
                    shasum: hashes.get(name).map(|s| s.to_owned()).unwrap_or_default(),
                },
            },
        );
    }

    Ok(())
}

async fn write_registry(path: impl AsRef<Path>, packages: BTreeMap<String, Package>) -> Result<()> {
    let registry = Registry {
        version: 1,
        packages: to_list(packages),
    };
    let json = serde_json::to_vec_pretty(&registry).expect("Failed to serialize registry to JSON.");

    fs::write(path, &json).await
}

fn calculate_hashes<'a>(
    path: &'a str,
    updated_plugins: &'a HashMap<String, String>,
) -> HashMap<&'a str, String> {
    info!("Calculating SHA512 hash of zip files...");

    updated_plugins
        .iter()
        .map(|(name, version)| -> std::io::Result<_> {
            let mut file = std::fs::File::open(format!("{}/{}_{}.zip", path, name, version))?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher)?;

            let hash = hasher.finalize();
            Ok((name.as_str(), format!("{:x}", hash)))
        })
        .filter_map(|s| s.ok())
        .collect()
}

pub async fn operate_registry<S: AsRef<str>>(
    path: &str,
    plugins_dir: S,
    updated: &HashMap<String, String>,
    i18n_store: &I18nStore,
) -> Result<()> {
    let hashes = calculate_hashes(&path, &updated);

    for lang in &["en", "zh_CN"] {
        let path = format!("{}/registry_{}.json", path, lang);
        let mut packages = read_registry(&path).await?;
        update_registry(
            &mut packages,
            &plugins_dir,
            updated.iter(),
            &hashes,
            lang,
            &i18n_store,
        )
        .await?;
        write_registry(&path, packages).await?;
    }

    Ok(())
}
