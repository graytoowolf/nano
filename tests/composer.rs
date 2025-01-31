// tests/composer.rs
use nano::composer;
use std::{collections::HashMap, env::temp_dir};
use tokio::{fs, io::Result};

#[tokio::test]
async fn parse_lock() -> Result<()> {
    let packages = composer::parse_lock("./tests/composer").await?;

    assert_eq!(
        packages.get("blessing/filter"),
        Some(&"v1.2.0".to_string())
    );

    Ok(())
}

#[tokio::test]
async fn run_composer() -> Result<()> {
    let mut path = temp_dir();
    path.push("composer-test");

    let _ = fs::remove_dir_all(&path).await;
    fs::create_dir_all(&path).await?;

    fs::write(
        path.join("composer.json"),
        r#"{ "require": { "php": "^8.1" } }"#,
    )
    .await?;

    composer::run_composer(&path).await
}

#[tokio::test]
async fn dedupe() -> Result<()> {
    let test_dir = temp_dir().join("dedupe-test");
    let path_display = test_dir.display();

    let _ = fs::remove_dir_all(&test_dir).await;
    fs::create_dir_all(&test_dir).await?;

    let mut bs_lock = HashMap::new();
    bs_lock.insert("illuminate/support".to_string(), "v6.20.0".to_string());
    bs_lock.insert("blessing/filter".to_string(), "v1.2.0".to_string());

    let composer_lock = serde_json::json!({
        "packages": [
            {
                "name": "illuminate/support",
                "version": "v6.20.0"
            },
            {
                "name": "local/package",
                "version": "v1.0.0"
            }
        ]
    });

    let composer_lock_path = test_dir.join("composer.lock");
    fs::write(&composer_lock_path, composer_lock.to_string()).await?;

    let composer_json_path = test_dir.join("composer.json");
    fs::write(&composer_json_path, b"{}").await?;

    let vendor_dir = test_dir.join("vendor");
    fs::create_dir(&vendor_dir).await?;

    // 创建测试目录结构
    let dirs = [
        vendor_dir.join("illuminate/support"),
        vendor_dir.join("blessing/filter"),
        vendor_dir.join("local/package"),
    ];
    for dir in &dirs {
        fs::create_dir_all(dir).await?;
        fs::write(dir.join("test.txt"), b"test").await?;
    }

    composer::dedupe(
        &bs_lock,
        &test_dir,
        &path_display,
        &composer_json_path.to_string_lossy(),
    )
    .await?;

    assert!(!composer_json_path.exists());
    assert!(!composer_lock_path.exists());
    assert!(!vendor_dir.join("illuminate/support").exists());
    assert!(vendor_dir.join("blessing/filter").exists());
    assert!(vendor_dir.join("local/package").exists());

    Ok(())
}