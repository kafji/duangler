use crate::{
    config::no_tls,
    protocol::{
        ClientMessage, HelloMessage, HelloReply, HelloReplyError, InputEvent, ServerMessage,
        Sha256, UpgradeTransportRequest, UpgradeTransportResponse,
    },
    transport::{
        generate_tls_key_pair, Certificate, PrivateKey, SingleCertVerifier, Transport, Transporter,
    },
};
use anyhow::{bail, Context, Error};
use futures::{future, FutureExt};
use std::{
    fmt::Debug,
    io::Write,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream},
    select,
    sync::mpsc::{self, error::SendError},
    task::{self, JoinError, JoinHandle},
};
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor, TlsStream};
use tracing::{debug, error, info, warn};

pub fn start(proto_event_rx: mpsc::Receiver<InputEvent>) -> JoinHandle<()> {
    task::spawn(async move { run(proto_event_rx).await.unwrap() })
}

async fn run(mut event_rx: mpsc::Receiver<InputEvent>) -> Result<(), Error> {
    let server_addr: SocketAddr = "0.0.0.0:3000".parse().context("invalid socket address")?;

    info!("listening at {}", server_addr);
    let listener = TcpListener::bind(server_addr).await?;

    let mut session_handler: Option<SessionHandler> = None;

    loop {
        let finished = session_handler
            .as_mut()
            .map(|x| x.finished().boxed())
            .unwrap_or_else(|| future::pending().boxed());

        select! { biased;

            // check if session is finished if it's exist
            x = finished => {
                if let Err(err) = x {
                    error!(?err);
                }
                session_handler.take();
            }

            // propagate to session if it's exist
            event = event_rx.recv() => {
                match (event, &mut session_handler) {
                    // propagate event to session
                    (Some(event), Some(session)) if session.is_connected() => { session.send_event(event).await.ok(); },
                    // stop server if channel is closed
                    (None, _) => break,
                    // drop event if we didn't have connected session
                    _ => (),
                }
            }

            // handle incoming connection, create a new session if it's not exist, otherwise drop the connection
            conn = listener.accept() => {
                let (stream, peer_addr) = conn?;
                info!(?peer_addr, "received incoming connection");
                if session_handler.is_none() {
                    let transporter = Transporter::Plain(Transport::new(stream));
                    let handler = spawn_session(peer_addr, transporter);
                    session_handler = Some(handler);
                }
            }
        }
    }

    Ok(())
}

/// Handler to a session.
#[derive(Debug)]
struct SessionHandler {
    event_tx: mpsc::Sender<InputEvent>,
    task: JoinHandle<()>,
    state: Arc<Mutex<State>>,
}

impl SessionHandler {
    /// Send input event to this session.
    async fn send_event(&mut self, event: InputEvent) -> Result<(), SendError<InputEvent>> {
        self.event_tx.send(event).await?;
        Ok(())
    }

    /// This method is cancel safe.
    async fn finished(&mut self) -> Result<(), JoinError> {
        (&mut self.task).await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        let state = self.state.lock().unwrap();
        match *state {
            State::Idle => true,
            State::ReceivedEvent { .. } => true,
            _ => false,
        }
    }
}

type ServerTransporter = Transporter<TcpStream, TlsStream<TcpStream>, ClientMessage, ServerMessage>;

#[derive(Debug)]
struct Session {
    peer_addr: SocketAddr,
    transporter: ServerTransporter,
    event_rx: mpsc::Receiver<InputEvent>,
    state: Arc<Mutex<State>>,
}

#[derive(Clone, Copy, Default, Debug)]
enum State {
    #[default]
    Handshaking,
    Idle,
    ReceivedEvent {
        event: InputEvent,
    },
}

/// Creates a new session.
fn spawn_session(peer_addr: SocketAddr, transporter: ServerTransporter) -> SessionHandler {
    let (event_tx, event_rx) = mpsc::channel(1);

    let state: Arc<Mutex<State>> = Default::default();

    let session = Session {
        peer_addr,
        transporter,
        event_rx,
        state: state.clone(),
    };

    let task = task::spawn(async move { run_session(session).await.unwrap() });

    SessionHandler {
        event_tx,
        task,
        state,
    }
}

/// The session loop.
async fn run_session(session: Session) -> Result<(), Error> {
    let Session {
        peer_addr,
        mut transporter,
        mut event_rx,
        state: state_ref,
    } = session;

    loop {
        // copy state from the mutex
        let state = {
            let state = state_ref.lock().unwrap();
            *state
        };

        debug!(?state);

        let new_state = match state {
            State::Handshaking => {
                let transport = transporter.plain()?;

                // wait for hello message
                let msg = transport.recv_msg().await?;
                let client_version =
                    if let ClientMessage::Hello(HelloMessage { client_version }) = msg {
                        client_version
                    } else {
                        bail!("expecting hello message, but was {:?}", msg);
                    };

                // check version
                let server_version = env!("CARGO_PKG_VERSION").to_owned();
                if client_version != server_version {
                    error!(?server_version, ?client_version, "version mismatch");

                    let msg: HelloReply = HelloReplyError::VersionMismatch.into();
                    transport.send_msg(msg.into()).await?;

                    break;
                }

                debug!("generating tls key pair");
                let (server_tls_cert, server_tls_key) =
                    generate_tls_key_pair("192.168.123.31".parse().unwrap())?;

                // request upgrade transport
                let msg: HelloReply = UpgradeTransportRequest {
                    server_tls_cert_hash: Sha256::from_bytes(server_tls_cert.as_ref()),
                }
                .into();
                transport.send_msg(msg.into()).await?;

                // wait for upgrade transport reply
                let msg = transport.recv_msg().await?;
                let client_tls_cert_hash =
                    if let ClientMessage::UpgradeTransportReply(UpgradeTransportResponse {
                        client_tls_cert_hash,
                    }) = msg
                    {
                        client_tls_cert_hash
                    } else {
                        bail!("expecting upgrade transport message, but was {:?}", msg);
                    };

                let prompt_answer = {
                    let client_tls_cert_hash = client_tls_cert_hash.clone();
                    task::spawn_blocking(move || {
                        let mut stdout = std::io::stdout();
                        write!(
                            stdout,
                            "Connect with client at {} and TLS certificate hash {}? [y/n]: ",
                            peer_addr, client_tls_cert_hash
                        )
                        .unwrap();
                        stdout.flush().unwrap();
                        let stdin = std::io::stdin();
                        let mut buf = String::new();
                        stdin
                            .read_line(&mut buf)
                            .expect("failed to read prompt answer");
                        buf.trim().to_lowercase() == "y"
                    })
                }
                .await?;

                if !prompt_answer {
                    break;
                }

                // upgrade to tls
                let no_tls = no_tls();
                if no_tls {
                    warn!("tls disabled");
                } else {
                    transporter = transporter
                        .upgrade(move |stream| {
                            upgrade_server_stream(
                                stream,
                                server_tls_cert,
                                server_tls_key,
                                client_tls_cert_hash,
                            )
                        })
                        .await?;
                }

                State::Idle
            }

            State::Idle => {
                let event = event_rx.recv().await;
                let event = match event {
                    Some(x) => x,
                    None => break,
                };

                State::ReceivedEvent { event }
            }

            State::ReceivedEvent { event } => {
                let transport = transporter.any();

                let msg = event.into();
                transport.send_msg(msg).await?;

                State::Idle
            }
        };

        // replace state in the mutex with the new state
        {
            let mut state = state_ref.lock().unwrap();
            *state = new_state;
        }
    }

    Result::<_, Error>::Ok(())
}

async fn upgrade_server_stream<S>(
    stream: S,
    server_tls_cert: Certificate,
    server_tls_key: PrivateKey,
    client_tls_cert_hash: Sha256,
) -> Result<TlsStream<S>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let tls: TlsAcceptor = {
        let client_cert_verifier = Arc::new(SingleCertVerifier::new(client_tls_cert_hash));

        let server_cert = rustls::Certificate(server_tls_cert.into());
        let server_private_key = rustls::PrivateKey(server_tls_key.into());

        let cfg = ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(client_cert_verifier)
            .with_single_cert(vec![server_cert], server_private_key)
            .context("failed to create server config tls")?;
        Arc::new(cfg).into()
    };

    let stream = tls.accept(stream).await.context("tls accept failed")?;

    Ok(stream.into())
}
