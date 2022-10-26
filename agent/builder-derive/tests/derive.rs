use builder_derive::Builder;
use configuration_derive::Configuration;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[crd("Test")]
struct SampleTest {
    bar: i32,
    biz: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[crd("Resource")]
struct SampleResource {
    bar: i32,
    biz: Option<String>,
}

#[test]
fn test_derive_1() {
    let test1 = SampleTest::builder()
        .biz("biz original".to_string())
        .bar(32)
        .biz(Some("biz updated".into()))
        .image("test1 image")
        .depends_on("my test")
        .build("test1")
        .unwrap();
    assert_eq!(
        test1
            .spec
            .agent
            .configuration
            .as_ref()
            .unwrap()
            .get("biz")
            .unwrap(),
        &serde_json::Value::String("biz updated".to_string())
    );
    assert_eq!(
        test1.spec.agent.configuration.unwrap().get("bar").unwrap(),
        &serde_json::Value::Number(32.into())
    );
    assert_eq!(test1.spec.agent.image, "test1 image".to_string());
    assert_eq!(test1.spec.depends_on, Some(vec!["my test".to_string()]));
    assert_eq!(test1.metadata.name.unwrap(), "test1".to_string());
    assert!(test1.spec.resources.is_empty());
}

#[test]
fn test_derive_2() {
    let test2 = SampleTest::builder()
        .bar_template("resource", "field")
        .biz_template("resource", "biz")
        .image("test2 image")
        .set_depends_on(Some(vec!["test1".to_string()]))
        .resources("resource")
        .build("test2")
        .unwrap();
    assert_eq!(
        test2
            .spec
            .agent
            .configuration
            .as_ref()
            .unwrap()
            .get("bar")
            .unwrap(),
        &serde_json::Value::String("${resource.field}".to_string())
    );
    assert_eq!(
        test2.spec.agent.configuration.unwrap().get("biz").unwrap(),
        &serde_json::Value::String("${resource.biz}".to_string())
    );
    assert_eq!(test2.spec.agent.image, "test2 image".to_string());
    assert_eq!(test2.spec.depends_on, Some(vec!["test1".to_string()]));
    assert_eq!(test2.metadata.name.unwrap(), "test2".to_string());
    assert_eq!(test2.spec.resources, vec!["resource".to_string()]);
}

#[test]
fn resource_derive_1() {
    let resource = SampleResource::builder()
        .biz("biz original".to_string())
        .bar(32)
        .biz("biz updated".to_string())
        .image("resource image")
        .depends_on("my resource")
        .build("resource")
        .unwrap();
    assert_eq!(
        resource
            .spec
            .agent
            .configuration
            .as_ref()
            .unwrap()
            .get("biz")
            .unwrap(),
        &serde_json::Value::String("biz updated".to_string())
    );
    assert_eq!(
        resource
            .spec
            .agent
            .configuration
            .unwrap()
            .get("bar")
            .unwrap(),
        &serde_json::Value::Number(32.into())
    );
    assert_eq!(resource.spec.agent.image, "resource image".to_string());
    assert_eq!(
        resource.spec.depends_on,
        Some(vec!["my resource".to_string()])
    );
    assert_eq!(resource.metadata.name.unwrap(), "resource".to_string());
}
