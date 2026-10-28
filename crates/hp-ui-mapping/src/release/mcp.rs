use hp_app::AppState;
use hp_fs::deps::DepRoot;

pub fn dependency_roots(state: &AppState) -> Vec<DepRoot> {
    super::collect_roots(state, &super::key_kinds(state.assets.fgd())).0
}

#[cfg(test)]
mod tests {
    #[test]
    fn roots_of_the_test_map() {
        let state = hp_app::testing::test_state();
        let roots = super::dependency_roots(&state);
        assert!(roots.iter().any(|r| r.name.to_ascii_lowercase().contains("concretefloor001a")));
    }
}
