#[test]
#[ignore = "requires explicit real-provider invocation and KG_EVAL_* environment variables"]
fn gold_fixtures_match_expected_extraction_and_graph_outputs() {
    kg_tdd::eval_support::evaluate_gold_fixtures_from_env()
        .expect("gold fixture evaluation against real provider");
}
