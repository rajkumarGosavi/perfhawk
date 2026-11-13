use prism_model::Timeline;

fn t(b: Vec<u32>) -> Timeline {
    Timeline {
        bucket_ns: 20_000_000,
        start_unix_ns: 0,
        sample_bucket: b,
    }
}

#[test]
fn bucket_count_is_max_plus_one() {
    assert_eq!(t(vec![]).bucket_count(), 0);
    assert_eq!(t(vec![0]).bucket_count(), 1);
    assert_eq!(t(vec![3, 1]).bucket_count(), 4);
}

#[test]
fn validate_checks_length() {
    assert!(t(vec![0, 1]).validate(2));
    assert!(!t(vec![0, 1]).validate(3));
    assert!(t(vec![]).validate(0));
}
