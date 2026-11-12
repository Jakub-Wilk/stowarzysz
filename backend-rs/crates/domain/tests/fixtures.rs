//! The differential fixtures are present and well-formed. The ports that must match them
//! (money, tallies, settlement, draws) add their own tests in later phases.

mod differential;

#[test]
fn every_fixture_file_has_cases_with_inputs_and_outputs() {
    for name in differential::FILES {
        let cases = differential::cases(name);
        assert!(cases.len() >= 40, "{name}: only {} cases", cases.len());
        for case in &cases {
            assert!(
                case.get("output").is_some(),
                "{name}: a case has no output: {case}"
            );
        }
    }
}

#[test]
fn the_pinned_conversion_example_is_there() {
    let first = &differential::cases("breakdown_to_base")[0];
    assert_eq!(first["rate"], "4.2537");
    assert_eq!(
        differential::pairs(&first["output"]["owed"]),
        [(1, 14182), (2, 14178), (3, 14177)]
    );
}
