mod raft_common;

use std::sync::Arc;
use std::time::Duration;

use quiccrunch_core::raft::{
    AppendEntriesRequest, InMemoryRaftTransport, InstallSnapshotChunk, RaftPeer, RaftTransport,
    RequestVoteRequest, SnapshotUpload,
};
use raft_common::TestNode;

#[tokio::test]
async fn in_memory_transport_implements_the_same_raft_seam() {
    let peer0 = RaftPeer::new(0, "memory://0");
    let peer1 = RaftPeer::new(1, "memory://1");
    let node0 = TestNode::new(0, 7, peer0.addr.clone(), vec![peer1.clone()]);
    let node1 = TestNode::new(1, 7, peer1.addr.clone(), vec![peer0.clone()]);

    let transport = Arc::new(InMemoryRaftTransport::new());
    transport.register(0, node0.clone()).await;
    transport.register(1, node1.clone()).await;

    assert!(
        node0
            .campaign(transport.as_ref(), std::slice::from_ref(&peer1))
            .await
    );
    node0
        .propose(
            transport.as_ref(),
            std::slice::from_ref(&peer1),
            b"x".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(node1.log_data().await, vec![b"x".to_vec()]);

    let vote = transport
        .request_vote(
            &peer1,
            RequestVoteRequest {
                partition_id: 7,
                term: node0.term().await,
                candidate_id: 0,
                last_log_index: 1,
                last_log_term: 1,
            },
        )
        .await
        .unwrap();
    assert!(vote.vote_granted);

    let chunk = InstallSnapshotChunk {
        partition_id: 7,
        term: node0.term().await,
        leader_id: 0,
        last_index: 10,
        last_term: 1,
        offset: 0,
        chunk: b"snapshot".to_vec(),
        done: true,
    };
    let ack = transport
        .install_snapshot_chunk(&peer1, chunk)
        .await
        .unwrap();
    assert!(ack.done);
    assert_eq!(ack.next_offset, 8);

    let upload = SnapshotUpload::new(7, node0.term().await, 0, 20, 1, vec![9; 40]);
    let ack = transport.install_snapshot(&peer1, upload).await.unwrap();
    assert!(ack.done);
    assert_eq!(node1.snapshot().await, vec![9; 40]);

    assert!(transport.metrics().append_entries_total >= 1);
    tokio::time::sleep(Duration::from_millis(1)).await;
}
