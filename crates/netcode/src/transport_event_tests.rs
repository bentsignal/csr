use super::*;

fn batch(count: usize) -> EventBatch {
    EventBatch {
        tick: 0,
        events: vec![sim::GameEvent::Jump { player: 1 }; count],
    }
}

#[test]
fn direct_queue_counts_multiple_batches_and_rejects_overflow_atomically() {
    let mut client = Client::probe("127.0.0.1:9".parse().unwrap()).unwrap();
    client.queue_events(batch(3)).unwrap();
    client.queue_events(batch(4)).unwrap();
    client.queue_events(batch(MAX_EVENTS - 7)).unwrap();
    assert_eq!(client.queued_events, MAX_EVENTS);
    assert_eq!(client.events.len(), 3);
    let before = client.events.clone();
    assert!(client.queue_events(batch(1)).is_err());
    assert_eq!(client.events, before);
    assert_eq!(client.queued_events, MAX_EVENTS);
    assert_eq!(client.take_events().unwrap().events.len(), 3);
    assert_eq!(client.queued_events, MAX_EVENTS - 3);
    client.queue_events(batch(3)).unwrap();
    assert_eq!(client.queued_events, MAX_EVENTS);
    while client.take_events().is_some() {}
    assert_eq!(client.queued_events, 0);
    client.queue_events(batch(2)).unwrap();
    client.disconnect();
    assert!(client.take_events().is_none());
    assert_eq!(client.queued_events, 0);
}

#[test]
fn malformed_late_receipt_does_not_enqueue_valid_prefix_or_change_accounting() {
    let mut client = Client::probe("127.0.0.1:9".parse().unwrap()).unwrap();
    client.queue_events(batch(2)).unwrap();
    let mut invalid = batch(1);
    let mut death = test_death_event();
    if let sim::GameEvent::Kill { death, .. } = &mut death {
        death.pose.origin.x = f32::NAN;
    }
    invalid.events.push(death);
    assert!(client.queue_events(invalid).is_err());
    assert_eq!(client.events.len(), 1);
    assert_eq!(client.queued_events, 2);
    assert_eq!(client.take_events().unwrap(), batch(2));
    assert!(client.take_events().is_none());
    assert_eq!(client.queued_events, 0);
}

#[test]
fn actual_udp_aggregate_event_overflow_is_terminal_and_clears_old_batches() {
    let content = ContentManifest {
        map_id: "csgo_map_events".into(),
        map_name: "events".into(),
        map_hash: "0".repeat(40),
        catalog_hash: "1".repeat(40),
    };
    let world = sim::World::new(Arc::new(collision::CollisionWorld::new()));
    let catalog = world.catalog.clone();
    let mut server = Server::bind(
        "127.0.0.1:0".parse().unwrap(),
        Session::new(world, content.clone()).unwrap(),
        2,
    )
    .unwrap();
    let mut client = Client::connect(
        server.address(),
        "Reader".into(),
        Team::CounterTerrorist,
        content,
        catalog,
    )
    .unwrap();
    let dt = Duration::from_secs_f64(1. / 128.);
    for _ in 0..256 {
        client.update(dt).unwrap();
        server.update(dt).unwrap();
        if client.player().is_some() {
            break;
        }
    }
    assert!(client.player().is_some());
    while client.take_events().is_some() {}
    server.inject_events(&batch(MAX_EVENTS / 2));
    server.inject_events(&batch(MAX_EVENTS / 2));
    for _ in 0..64 {
        server.update(dt).unwrap();
        client.update(dt).unwrap();
        if client.queued_events == MAX_EVENTS {
            break;
        }
    }
    assert_eq!(client.queued_events, MAX_EVENTS);
    assert_eq!(client.events.len(), 2); // previous batch-only cap never catches this
    server.inject_events(&batch(1));
    let mut failed = false;
    for _ in 0..64 {
        server.update(dt).unwrap();
        if client.update(dt).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
    assert!(client.failure().unwrap().contains("not consuming"));
    assert!(client.take_events().is_none());
    assert_eq!(client.queued_events, 0);
    assert!(client.send_input(TickInput::default(), 0).is_err());
}
