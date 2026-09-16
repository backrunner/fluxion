use std::{collections::HashMap, sync::LazyLock};
static DICTIONARIES: LazyLock<HashMap<&'static str, HashMap<String, String>>> =
    LazyLock::new(|| {
        [
            ("en", include_str!("../assets/locales/en.json")),
            ("zh-CN", include_str!("../assets/locales/zh-CN.json")),
            ("ja", include_str!("../assets/locales/ja.json")),
            ("ko", include_str!("../assets/locales/ko.json")),
        ]
        .into_iter()
        .map(|(key, value)| {
            (
                key,
                serde_json::from_str(value).expect("embedded translations"),
            )
        })
        .collect()
    });
pub fn t(locale: &str, key: &str) -> String {
    DICTIONARIES
        .get(locale)
        .or_else(|| DICTIONARIES.get("en"))
        .and_then(|d| d.get(key))
        .cloned()
        .unwrap_or_else(|| key.into())
}
