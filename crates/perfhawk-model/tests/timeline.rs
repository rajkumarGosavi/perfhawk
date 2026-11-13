use perfhawk_model::Timeline;

fn t(b: Vec<u32>) -> Timeline {
    Timeline {
        bucket_ns: 20_000_000,
        start_unix_ns: 0,
        total_buckets: 0,
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

#[test]
fn bucket_count_includes_trailing_idle_time() {
    let mut x = t(vec![0, 3]);
    x.total_buckets = 100;
    assert_eq!(x.bucket_count(), 100);
    x.total_buckets = 2;
    assert_eq!(x.bucket_count(), 4);
    let mut e = t(vec![]);
    e.total_buckets = 50;
    assert_eq!(e.bucket_count(), 50);
}
