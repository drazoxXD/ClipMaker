//! The backend's own strings (tray menu) come from the same JSON files the UI uses.

use serde_json::Value;
use std::sync::OnceLock;

fn dicts() -> &'static [(&'static str, Value)] {
	static D: OnceLock<Vec<(&'static str, Value)>> = OnceLock::new();
	D.get_or_init(|| {
		vec![
			("en", serde_json::from_str(include_str!("../../src/locales/en.json")).unwrap()),
			("hu", serde_json::from_str(include_str!("../../src/locales/hu.json")).unwrap()),
		]
	})
}

fn lookup<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
	key.split('.').try_fold(v, |node, part| node.get(part))?.as_str()
}

pub fn t(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
	let ds = dicts();
	let en = &ds[0].1;
	let dict = ds.iter().find(|(l, _)| *l == lang).map(|(_, d)| d).unwrap_or(en);
	let mut s = lookup(dict, key).or_else(|| lookup(en, key)).unwrap_or(key).to_string();
	for (k, v) in vars {
		s = s.replace(&format!("{{{k}}}"), v);
	}
	s
}
