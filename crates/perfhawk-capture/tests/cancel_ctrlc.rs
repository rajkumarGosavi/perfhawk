use perfhawk_capture::CancelToken;

#[test]
fn a_second_install_still_gets_the_interrupt() {
    // `doctor --follow-up` installs a token for the Doctor capture and another
    // for the follow-up; Ctrl+C during the second must still cancel it.
    let first = CancelToken::install_ctrlc();
    let second = CancelToken::install_ctrlc();
    perfhawk_capture::cancel::fire_installed();
    assert!(second.is_cancelled());
    assert!(!first.is_cancelled());
}
