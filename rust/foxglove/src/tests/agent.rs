use foxglove_agent::foxglove_add;

#[test]
fn test_agent() {
    let result = unsafe { foxglove_add(1, 2) };
    assert_eq!(result, 3);
}
