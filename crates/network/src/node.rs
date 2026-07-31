use anyhow::Result;
use libp2p::{
    Multiaddr, SwarmBuilder, futures::StreamExt, gossipsub, identify, kad, mdns, noise,
    swarm::SwarmEvent, tcp, yamux,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use crate::cache::{CachedSdf, SdfCache};
use crate::cdn::VivaldiCoord;
use crate::identity::Identity;
use crate::sync::{SyncEvent, TOPIC_SDF_EVENTS};
use crate::vcs::SdfDag;

/// ALICE P2P ネットワークノード
pub struct AliceNode {
    pub identity: Identity,
    pub cache: Arc<Mutex<SdfCache>>,
    pub dag: SdfDag,
    pub coord: VivaldiCoord,
    publish_tx: Option<mpsc::Sender<SyncEvent>>,
    #[allow(dead_code)]
    shutdown_tx: Option<mpsc::Sender<()>>,
    data_dir: std::path::PathBuf,
}

impl AliceNode {
    pub fn init(data_dir: &Path) -> Result<Self> {
        info!("initializing ALICE node");

        let identity = Identity::load_or_create(data_dir)?;
        info!(did = %identity.did.id, "identity loaded");

        let cache = SdfCache::open(data_dir)?;
        info!(cached = cache.len(), "cache loaded");

        let dag_path = data_dir.join("dag.json");
        let dag = SdfDag::load_or_new(&dag_path);
        info!(nodes = dag.node_count(), "DAG loaded");

        let coord = VivaldiCoord::origin();

        Ok(Self {
            identity,
            cache: Arc::new(Mutex::new(cache)),
            dag,
            coord,
            publish_tx: None,
            shutdown_tx: None,
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn start_background(&mut self, runtime: &tokio::runtime::Runtime) -> Result<()> {
        let did = self.identity.did.id.clone();
        let cache = Arc::clone(&self.cache);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let (publish_tx, publish_rx) = mpsc::channel::<SyncEvent>(64);

        self.shutdown_tx = Some(shutdown_tx);
        self.publish_tx = Some(publish_tx);

        runtime.spawn(async move {
            if let Err(e) = run_swarm(&did, cache, &mut shutdown_rx, publish_rx).await {
                tracing::warn!(error = %e, "P2P node stopped");
            }
        });

        info!(did = %self.identity.did.id, "P2P node started in background");
        Ok(())
    }

    pub fn publish_sdf(&self, id: &str, lol_source: &str, prompt: &str) {
        if let Some(tx) = &self.publish_tx {
            let event = SyncEvent::SdfPublished {
                id: id.to_string(),
                lol_source: lol_source.to_string(),
                author_did: self.identity.did.id.clone(),
                prompt: prompt.to_string(),
            };
            let _ = tx.try_send(event);
        }
    }

    pub fn fork_sdf(&mut self, original_hash: &str, new_lol: &str) {
        let did = self.identity.did.id.clone();
        if let Some((_node, diff)) = self.dag.fork(original_hash, new_lol, &did)
            && let Some(tx) = &self.publish_tx
        {
            let _ = tx.try_send(SyncEvent::SdfForked { diff });
        }
        self.save_dag();
    }

    pub fn list_cached_sdfs(&self) -> Vec<CachedSdf> {
        self.cache
            .lock()
            .map(|c| c.list_public().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn dag_node_count(&self) -> usize {
        self.dag.node_count()
    }

    fn save_dag(&self) {
        let path = self.data_dir.join("dag.json");
        if let Err(e) = self.dag.save(&path) {
            tracing::warn!(error = %e, "failed to save DAG");
        }
    }
}

async fn run_swarm(
    did: &str,
    cache: Arc<Mutex<SdfCache>>,
    shutdown_rx: &mut mpsc::Receiver<()>,
    mut publish_rx: mpsc::Receiver<SyncEvent>,
) -> Result<()> {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .build()
        .map_err(|e| anyhow::anyhow!("gossipsub config: {e}"))?;

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config.clone(),
            )
            .expect("gossipsub behaviour");

            let identify = identify::Behaviour::new(identify::Config::new(
                "/alice/0.1.0".to_string(),
                key.public(),
            ));

            let kademlia = kad::Behaviour::new(
                key.public().to_peer_id(),
                kad::store::MemoryStore::new(key.public().to_peer_id()),
            );

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())
                    .expect("mDNS behaviour");

            AliceBehaviour {
                gossipsub,
                identify,
                kademlia,
                mdns,
            }
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let topic = gossipsub::IdentTopic::new(TOPIC_SDF_EVENTS);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let listen_addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse()?;
    swarm.listen_on(listen_addr)?;

    info!(did, "P2P swarm listening");

    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%address, "P2P listening on");
                    }
                    // mDNS: LAN 内ノード自動検出
                    SwarmEvent::Behaviour(AliceBehaviourEvent::Mdns(
                        mdns::Event::Discovered(peers)
                    )) => {
                        for (peer_id, addr) in peers {
                            info!(%peer_id, %addr, "mDNS discovered peer");
                            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                            swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                        }
                    }
                    SwarmEvent::Behaviour(AliceBehaviourEvent::Mdns(
                        mdns::Event::Expired(peers)
                    )) => {
                        for (peer_id, _addr) in peers {
                            tracing::debug!(%peer_id, "mDNS peer expired");
                            swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                        }
                    }
                    // gossipsub メッセージ受信
                    SwarmEvent::Behaviour(AliceBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. }
                    )) => {
                        if let Ok(sync_event) = serde_json::from_slice::<SyncEvent>(&message.data) {
                            handle_sync_event(sync_event, &cache);
                        }
                    }
                    _ => {}
                }
            }
            Some(event) = publish_rx.recv() => {
                if let Ok(data) = serde_json::to_vec(&event) {
                    let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
                    tracing::debug!("published SDF event to P2P network");
                    handle_sync_event(event, &cache);
                }
            }
            _ = shutdown_rx.recv() => {
                info!("P2P node shutting down");
                break;
            }
        }
    }

    Ok(())
}

fn handle_sync_event(event: SyncEvent, cache: &Arc<Mutex<SdfCache>>) {
    match event {
        SyncEvent::SdfPublished {
            id,
            lol_source,
            author_did,
            prompt,
        } => {
            if alice_bamboo::parse_lol(&lol_source).is_err() {
                tracing::warn!(id, "received invalid LOL source, dropping");
                return;
            }

            let sdf = CachedSdf {
                id,
                lol_source,
                author_did,
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            if let Ok(mut c) = cache.lock() {
                let _ = c.put(sdf);
                tracing::debug!(cached = c.len(), "SDF cached from P2P");
            }

            let _ = prompt;
        }
        SyncEvent::SdfForked { diff } => {
            if alice_bamboo::parse_lol(&diff.forked_lol).is_err() {
                tracing::warn!(hash = %diff.fork_hash, "received invalid forked LOL, dropping");
                return;
            }

            let sdf = CachedSdf {
                id: diff.fork_hash.clone(),
                lol_source: diff.forked_lol.clone(),
                author_did: diff.author_did.clone(),
                created_at: diff.timestamp.clone(),
            };

            if let Ok(mut c) = cache.lock() {
                let _ = c.put(sdf);
                tracing::debug!(
                    fork = %diff.fork_hash,
                    parent = %diff.original_hash,
                    "forked SDF cached from P2P"
                );
            }
        }
    }
}

use libp2p::swarm::NetworkBehaviour;

#[derive(NetworkBehaviour)]
struct AliceBehaviour {
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    mdns: mdns::tokio::Behaviour,
}
