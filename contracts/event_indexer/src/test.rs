#![cfg(test)]

extern crate std;

use soroban_sdk::{
    symbol_short, testutils::Ledger, vec, Env, IntoVal, String, Symbol, Val, Vec,
};

use crate::{Error, EventIndexer, EventIndexerClient};

fn payload(e: &Env, price: i128, amount: i128) -> Vec<(Symbol, Val)> {
    vec![
        e,
        (symbol_short!("price"), price.into_val(e)),
        (symbol_short!("amount"), amount.into_val(e)),
    ]
}

#[test]
fn test_record_and_query() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);

    let contract_id = e.register(EventIndexer, ());
    let client = EventIndexerClient::new(&e, &contract_id);

    let topic = symbol_short!("payment");
    let key = String::from_str(&e, "claim-1");

    let seq1 = client.record(&topic, &symbol_short!("created"), &key, &payload(&e, 100, 10));
    let seq2 = client.record(&topic, &symbol_short!("confirmed"), &key, &payload(&e, 110, 10));

    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert_eq!(client.count(&topic), 2);

    let latest = client.latest(&topic).unwrap();
    assert_eq!(latest.seq, 2);
    assert_eq!(latest.event_name, symbol_short!("confirmed"));
    assert_eq!(latest.timestamp, 1000);

    let all = client.query(&topic, &1, &10);
    assert_eq!(all.len(), 2);
    assert_eq!(all.get(0).unwrap().seq, 1);
    assert_eq!(all.get(1).unwrap().seq, 2);

    let tail = client.query(&topic, &2, &10);
    assert_eq!(tail.len(), 1);
    assert_eq!(tail.get(0).unwrap().seq, 2);
}

#[test]
fn test_topics_are_independent() {
    let e = Env::default();
    let contract_id = e.register(EventIndexer, ());
    let client = EventIndexerClient::new(&e, &contract_id);

    client.record(
        &symbol_short!("payment"),
        &symbol_short!("created"),
        &String::from_str(&e, "k1"),
        &payload(&e, 1, 2),
    );
    client.record(
        &symbol_short!("aloop"),
        &symbol_short!("changed"),
        &String::from_str(&e, "k2"),
        &payload(&e, 3, 4),
    );

    assert_eq!(client.count(&symbol_short!("payment")), 1);
    assert_eq!(client.count(&symbol_short!("aloop")), 1);
}

#[test]
fn test_topic_limit_evicts_oldest() {
    let e = Env::default();
    let contract_id = e.register(EventIndexer, ());
    let client = EventIndexerClient::new(&e, &contract_id);

    let topic = symbol_short!("payment");
    assert_eq!(client.set_topic_limit(&topic, &2), ());
    assert_eq!(client.get_topic_limit(&topic), 2);

    for key_name in ["k1", "k2", "k3"] {
        client.record(
            &topic,
            &symbol_short!("created"),
            &String::from_str(&e, key_name),
            &payload(&e, 0, 0),
        );
    }

    assert_eq!(client.count(&topic), 2);
    let remaining = client.query(&topic, &1, &10);
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining.get(0).unwrap().seq, 2);
    assert_eq!(remaining.get(1).unwrap().seq, 3);
}

#[test]
fn test_empty_topic() {
    let e = Env::default();
    let contract_id = e.register(EventIndexer, ());
    let client = EventIndexerClient::new(&e, &contract_id);

    let topic = symbol_short!("nothing");
    assert_eq!(client.count(&topic), 0);
    assert_eq!(client.latest(&topic), None);
    assert_eq!(client.query(&topic, &1, &10).len(), 0);
}

#[test]
fn test_limit_validation() {
    let e = Env::default();
    let contract_id = e.register(EventIndexer, ());
    let client = EventIndexerClient::new(&e, &contract_id);

    let topic = symbol_short!("payment");
    assert_eq!(
        e.as_contract(&contract_id, || EventIndexer::set_topic_limit(
            e.clone(),
            topic.clone(),
            0
        )),
        Err(Error::LimitTooSmall)
    );
    assert_eq!(
        e.as_contract(&contract_id, || EventIndexer::query(e.clone(), topic.clone(), 1, 0)),
        Err(Error::InvalidQueryLimit)
    );
    // The rejected limit write must not have changed the stored limit.
    assert_eq!(client.get_topic_limit(&topic), crate::DEFAULT_MAX_EVENTS_PER_TOPIC);
}