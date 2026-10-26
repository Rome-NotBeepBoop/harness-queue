use harness_queue::models::ItemStatus;
use harness_queue::queue::QueueEngine;

#[test]
fn test_fifo_ordering() {
    let mut q = QueueEngine::new();
    assert_eq!(q.len(), 0);
    assert!(q.is_empty());

    let id1 = q.enqueue("Prompt 1".to_string());
    let id2 = q.enqueue("Prompt 2".to_string());
    let id3 = q.enqueue("Prompt 3".to_string());

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
    assert_eq!(q.len(), 3);

    // Peek next eligible for dispatch
    let next = q.peek_next_pending().expect("item 1");
    assert_eq!(next.id, 1);
    assert_eq!(next.prompt, "Prompt 1");

    // Mark submitting
    q.mark_submitting(1, 1000).expect("mark submitting 1");
    assert_eq!(q.get(1).unwrap().status, ItemStatus::Submitting);

    // Next pending is now prompt 2
    let next2 = q.peek_next_pending().expect("item 2");
    assert_eq!(next2.id, 2);

    // Acknowledge prompt 1
    q.mark_acknowledged(1).expect("mark ack 1");
    assert_eq!(q.get(1).unwrap().status, ItemStatus::Acknowledged);

    // Next pending remains prompt 2
    assert_eq!(q.peek_next_pending().unwrap().id, 2);
}

#[test]
fn test_deduplication_and_no_double_submit() {
    let mut q = QueueEngine::new();
    q.enqueue("First".to_string());
    q.mark_submitting(1, 1000).unwrap();

    // Re-marking an already submitting item fails
    assert!(q.mark_submitting(1, 1001).is_err());

    // Acknowledging once succeeds, acknowledging again fails
    q.mark_acknowledged(1).unwrap();
    assert!(q.mark_acknowledged(1).is_err());
}
