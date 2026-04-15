#[test]
#[ignore = "requires explicit real-provider invocation and KG_EVAL_* environment variables"]
fn gold_fixtures_match_expected_extraction_and_graph_outputs() {
    kg_tdd::init_tracing_for_process();
    kg_tdd::eval_support::evaluate_gold_fixtures_from_env()
        .expect("gold fixture evaluation against real provider");
}

#[test]
#[ignore = "requires explicit real-provider invocation and KG_EVAL_* environment variables"]
fn gold_fixtures_remain_stable_across_repeated_runs() {
    kg_tdd::init_tracing_for_process();
    kg_tdd::eval_support::evaluate_gold_fixtures_repeatedly_from_env()
        .expect("repeated gold fixture evaluation against real provider");
}
