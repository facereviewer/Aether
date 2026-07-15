use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use crate::error::{AetherError, Result};
use crate::netstack::StackHandle;

const SOCKS_VER: u8 = 0x05;
const SOCKS_CMD_CONNECT: u8 = 0x01;
const SOCKS_CMD_UDP_ASSOCIATE: u8 = 0x03;
const SOCKS_ATYP_V4: u8 = 0x01;
const SOCKS_ATYP_DOMAIN: u8 = 0x03;
const SOCKS_ATYP_V6: u8 = 0x04;
const SOCKS_REP_OK: u8 = 0x00;
const SOCKS_REP_GENERAL: u8 = 0x01;
const SOCKS_REP_NOT_SUPPORTED: u8 = 0x07;

#[derive(Clone)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

enum Target {
    Ip(IpAddr),
    Domain(String),
}

pub struct MixedProxyConfig {
    pub listen: SocketAddr,
    pub auth: Option<ProxyAuth>,
}

pub async fn serve(config: MixedProxyConfig, stack: StackHandle) -> Result<()> {
    let listener = TcpListener::bind(config.listen).await?;
    log::info!("mixed proxy listening on {}", config.listen);
    if config.auth.is_some() {
        log::info!("authentication enabled");
    }

    loop {
        let (sock, peer) = listener.accept().await?;
        let stack = stack.clone();
        let auth = config.auth.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(sock, stack, auth).await {
                log::debug!("mixed proxy client {peer} ended: {e}");
            }
        });
    }
}

async fn handle_connection(
    sock: TcpStream,
    stack: StackHandle,
    auth: Option<ProxyAuth>,
) -> Result<()> {
    let mut first_byte = [0u8; 1];
    sock.peek(&mut first_byte).await?;

    if first_byte[0] == SOCKS_VER {
        handle_socks5(sock, stack, auth).await
    } else {
        handle_http(sock, stack, auth).await
    }
}

// ==================== SOCKS5 ====================

async fn handle_socks5(
    mut sock: TcpStream,
    stack: StackHandle,
    auth: Option<ProxyAuth>,
) -> Result<()> {
    let mut prefix = [0u8; 2];
    sock.read_exact(&mut prefix).await?;
    if prefix[0] != SOCKS_VER {
        return Err(AetherError::Other("bad socks version".into()));
    }
    let nmethods = prefix[1] as usize;
    let mut methods = vec![0u8; nmethods];
    sock.read_exact(&mut methods).await?;

    if auth.is_some() {
        if !methods.contains(&0x02) {
            sock.write_all(&[SOCKS_VER, 0xFF]).await?;
            return Err(AetherError::Other("client doesn't support auth".into()));
        }
        sock.write_all(&[SOCKS_VER, 0x02]).await?;
        let mut auth_ver = [0u8; 1];
        sock.read_exact(&mut auth_ver).await?;
        if auth_ver[0] != 0x01 {
            return Err(AetherError::Other("bad auth version".into()));
        }
        let ulen = { let mut b = [0u8; 1]; sock.read_exact(&mut b).await?; b[0] as usize };
        let mut username = vec![0u8; ulen];
        sock.read_exact(&mut username).await?;
        let plen = { let mut b = [0u8; 1]; sock.read_exact(&mut b).await?; b[0] as usize };
        let mut password = vec![0u8; plen];
        sock.read_exact(&mut password).await?;

        if let Some(ref a) = auth {
            if username != a.username.as_bytes() || password != a.password.as_bytes() {
                sock.write_all(&[0x01, 0x01]).await?;
                return Err(AetherError::Other("auth failed".into()));
            }
        }
        sock.write_all(&[0x01, 0x00]).await?;
    } else {
        sock.write_all(&[SOCKS_VER, 0x00]).await?;
    }

    let mut head = [0u8; 4];
    sock.read_exact(&mut head).await?;
    if head[0] != SOCKS_VER {
        return Err(AetherError::Other("bad socks request".into()));
    }

    let cmd = head[1];
    let atyp = head[3];
    let (target, port) = read_socks_target(&mut sock, atyp).await?;

    match cmd {
        SOCKS_CMD_CONNECT => {
            let ip = match resolve_target(&stack, target).await {
                Ok(ip) => ip,
                Err(e) => { let _ = socks_reply(&mut sock, SOCKS_REP_GENERAL).await; return Err(e); }
            };
            let dst = SocketAddr::new(ip, port);
            let conn = match stack.open_tcp(dst).await {
                Ok(c) => c,
                Err(e) => { let _ = socks_reply(&mut sock, SOCKS_REP_GENERAL).await; return Err(e); }
            };
            socks_reply_bound(&mut sock, "0.0.0.0:0".parse().unwrap()).await?;
            bridge_tcp(sock, conn).await
        }
        SOCKS_CMD_UDP_ASSOCIATE => {
            let relay = UdpSocket::bind("127.0.0.1:0").await?;
            let relay_addr = relay.local_addr()?;
            socks_reply_bound(&mut sock, relay_addr).await?;
            let udp = stack.open_udp().await?;
            let (sender, mut from_stack) = udp.into_split();
            let mut client: Option<SocketAddr> = None;
            let mut cbuf = vec![0u8; 65535];
            let mut ctrl = [0u8; 256];
            loop {
                tokio::select! {
                    r = relay.recv_from(&mut cbuf) => {
                        let (n, from) = match r { Ok(v) => v, Err(_) => break };
                        client = Some(from);
                        if let Some((dst, payload)) = parse_socks_udp(&cbuf[..n]) {
                            let dst = match dst {
                                Target::Ip(ip) => SocketAddr::new(ip, payload.0),
                                Target::Domain(name) => {
                                    match dns_resolve(&stack, &name).await {
                                        Ok(ip) => SocketAddr::new(ip, payload.0),
                                        Err(_) => continue,
                                    }
                                }
                            };
                            let _ = sender.send_to(dst, payload.1).await;
                        }
                    }
                    maybe = from_stack.recv() => {
                        let (src, data) = match maybe { Some(v) => v, None => break };
                        if let Some(c) = client {
                            let pkt = build_socks_udp_reply(src, &data);
                            let _ = relay.send_to(&pkt, c).await;
                        }
                    }
                    r = sock.read(&mut ctrl) => {
                        match r { Ok(0) | Err(_) => break, Ok(_) => {} }
                    }
                }
            }
            sender.close().await;
            Ok(())
        }
        _ => {
            let _ = socks_reply(&mut sock, SOCKS_REP_NOT_SUPPORTED).await;
            Err(AetherError::Other("unsupported socks command".into()))
        }
    }
}

async fn read_socks_target(sock: &mut TcpStream, atyp: u8) -> Result<(Target, u16)> {
    let target = match atyp {
        SOCKS_ATYP_V4 => {
            let mut b = [0u8; 4];
            sock.read_exact(&mut b).await?;
            Target::Ip(IpAddr::V4(Ipv4Addr::from(b)))
        }
        SOCKS_ATYP_V6 => {
            let mut b = [0u8; 16];
            sock.read_exact(&mut b).await?;
            Target::Ip(IpAddr::V6(b.into()))
        }
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0u8; 1];
            sock.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            sock.read_exact(&mut name).await?;
            Target::Domain(String::from_utf8_lossy(&name).to_string())
        }
        _ => return Err(AetherError::Other("bad atyp".into())),
    };
    let mut port = [0u8; 2];
    sock.read_exact(&mut port).await?;
    Ok((target, u16::from_be_bytes(port)))
}

async fn socks_reply(sock: &mut TcpStream, code: u8) -> Result<()> {
    sock.write_all(&[SOCKS_VER, code, 0x00, SOCKS_ATYP_V4, 0, 0, 0, 0, 0, 0]).await?;
    Ok(())
}

async fn socks_reply_bound(sock: &mut TcpStream, bound: SocketAddr) -> Result<()> {
    let mut buf = vec![SOCKS_VER, SOCKS_REP_OK, 0x00];
    match bound.ip() {
        IpAddr::V4(v4) => { buf.push(SOCKS_ATYP_V4); buf.extend_from_slice(&v4.octets()); }
        IpAddr::V6(v6) => { buf.push(SOCKS_ATYP_V6); buf.extend_from_slice(&v6.octets()); }
    }
    buf.extend_from_slice(&bound.port().to_be_bytes());
    sock.write_all(&buf).await?;
    Ok(())
}

async fn resolve_target(stack: &StackHandle, target: Target) -> Result<IpAddr> {
    match target {
        Target::Ip(ip) => Ok(ip),
        Target::Domain(name) => {
            if let Ok(ip) = name.parse::<IpAddr>() { return Ok(ip); }
            dns_resolve(stack, &name).await
        }
    }
}

async fn dns_resolve(stack: &StackHandle, name: &str) -> Result<IpAddr> {
    let udp = stack.open_udp().await?;
    let server: SocketAddr = "1.1.1.1:53".parse().unwrap();
    let query = build_dns_query(name, 1);
    udp.send_to(server, query).await?;
    let (_sender, mut from_stack) = udp.into_split();
    let resp = tokio::time::timeout(Duration::from_secs(5), from_stack.recv())
        .await
        .map_err(|_| AetherError::Other("dns timeout".into()))?
        .ok_or_else(|| AetherError::Other("dns channel closed".into()))?;
    parse_dns_a(&resp.1).ok_or_else(|| AetherError::Other(format!("no A record for {name}")))
}

fn build_dns_query(name: &str, qtype: u16) -> Vec<u8> {
    let mut q = Vec::with_capacity(32 + name.len());
    let id: u16 = rand::random();
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    for label in name.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0x00);
    q.extend_from_slice(&qtype.to_be_bytes());
    q.extend_from_slice(&[0x00, 0x01]);
    q
}

fn parse_dns_a(resp: &[u8]) -> Option<IpAddr> {
    if resp.len() < 12 { return None; }
    let qd = u16::from_be_bytes([resp[4], resp[5]]) as usize;
    let mut pos = 12;
    for _ in 0..qd {
        pos = skip_name(resp, pos)?;
        pos = pos.checked_add(4)?;
    }
    let an = u16::from_be_bytes([resp[6], resp[7]]) as usize;
    for _ in 0..an {
        pos = skip_name(resp, pos)?;
        if pos + 10 > resp.len() { return None; }
        let rtype = u16::from_be_bytes([resp[pos], resp[pos + 1]]);
        let rdlen = u16::from_be_bytes([resp[pos + 8], resp[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlen > resp.len() { return None; }
        if rtype == 1 && rdlen == 4 {
            return Some(IpAddr::V4(Ipv4Addr::new(resp[pos], resp[pos+1], resp[pos+2], resp[pos+3])));
        }
        pos += rdlen;
    }
    None
}

fn skip_name(buf: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = *buf.get(pos)?;
        if len & 0xc0 == 0xc0 { return Some(pos + 2); }
        if len == 0 { return Some(pos + 1); }
        pos += 1 + len as usize;
    }
}

fn parse_socks_udp(buf: &[u8]) -> Option<(Target, (u16, Vec<u8>))> {
    if buf.len() < 4 || buf[2] != 0 { return None; }
    let atyp = buf[3];
    let mut pos = 4;
    let target = match atyp {
        SOCKS_ATYP_V4 => {
            if buf.len() < pos + 4 { return None; }
            let ip = Ipv4Addr::new(buf[pos], buf[pos+1], buf[pos+2], buf[pos+3]);
            pos += 4;
            Target::Ip(IpAddr::V4(ip))
        }
        SOCKS_ATYP_V6 => {
            if buf.len() < pos + 16 { return None; }
            let mut b = [0u8; 16];
            b.copy_from_slice(&buf[pos..pos+16]);
            pos += 16;
            Target::Ip(IpAddr::V6(b.into()))
        }
        SOCKS_ATYP_DOMAIN => {
            let len = *buf.get(pos)? as usize;
            pos += 1;
            if buf.len() < pos + len { return None; }
            let name = String::from_utf8_lossy(&buf[pos..pos+len]).to_string();
            pos += len;
            Target::Domain(name)
        }
        _ => return None,
    };
    if buf.len() < pos + 2 { return None; }
    let port = u16::from_be_bytes([buf[pos], buf[pos+1]]);
    pos += 2;
    Some((target, (port, buf[pos..].to_vec())))
}

fn build_socks_udp_reply(src: SocketAddr, data: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00];
    match src.ip() {
        IpAddr::V4(v4) => { pkt.push(SOCKS_ATYP_V4); pkt.extend_from_slice(&v4.octets()); }
        IpAddr::V6(v6) => { pkt.push(SOCKS_ATYP_V6); pkt.extend_from_slice(&v6.octets()); }
    }
    pkt.extend_from_slice(&src.port().to_be_bytes());
    pkt.extend_from_slice(data);
    pkt
}

// ==================== HTTP CONNECT ====================

async fn handle_http(
    mut sock: TcpStream,
    stack: StackHandle,
    auth: Option<ProxyAuth>,
) -> Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1024];

    loop {
        let n = sock.read(&mut tmp).await?;
        if n == 0 { return Err(AetherError::Other("http: connection closed".into())); }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") { break; }
        if buf.len() > 8192 { return Err(AetherError::Other("http: headers too large".into())); }
    }

    let request = String::from_utf8_lossy(&buf);
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 { return Err(AetherError::Other("http: bad request".into())); }

    let method = parts[0];
    let target = parts[1];

    if let Some(ref a) = auth {
        let mut authorized = false;
        for line in request.lines() {
            if line.to_lowercase().starts_with("proxy-authorization:") {
                let val = line[20..].trim();
                if let Some(cred) = val.strip_prefix("Basic ") {
                    use base64::Engine;
                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(cred) {
                        let decoded = String::from_utf8_lossy(&decoded);
                        if let Some((user, pass)) = decoded.split_once(':') {
                            if user == a.username && pass == a.password { authorized = true; }
                        }
                    }
                }
                break;
            }
        }
        if !authorized {
            let resp = "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"aether\"\r\nContent-Length: 0\r\n\r\n";
            sock.write_all(resp.as_bytes()).await?;
            return Err(AetherError::Other("http: auth required".into()));
        }
    }

    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = if let Some((h, p)) = target.rsplit_once(':') {
            (h, p.parse::<u16>().unwrap_or(443))
        } else {
            (target, 443)
        };

        let ip = if let Ok(ip) = host.parse::<IpAddr>() { ip } else { dns_resolve(&stack, host).await? };
        let dst = SocketAddr::new(ip, port);
        let conn = match stack.open_tcp(dst).await {
            Ok(c) => c,
            Err(e) => {
                let _ = sock.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await;
                return Err(e);
            }
        };
        sock.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
        bridge_tcp(sock, conn).await
    } else {
        sock.write_all(b"HTTP/1.1 501 Not Implemented\r\nContent-Length: 0\r\n\r\n").await?;
        Err(AetherError::Other("http: only CONNECT supported".into()))
    }
}

// ==================== Bridge ====================

async fn bridge_tcp(client: TcpStream, conn: crate::netstack::TcpConn) -> Result<()> {
    let (sender, mut from_stack) = conn.into_split();
    let (mut rd, mut wr) = client.into_split();

    let up = tokio::spawn(async move {
        let mut buf = vec![0u8; 16384];
        loop {
            match rd.read(&mut buf).await {
                Ok(0) => { sender.close().await; break; }
                Ok(n) => { if sender.send(buf[..n].to_vec()).await.is_err() { break; } }
                Err(_) => { sender.close().await; break; }
            }
        }
    });

    while let Some(data) = from_stack.recv().await {
        if wr.write_all(&data).await.is_err() { break; }
    }

    let _ = wr.shutdown().await;
    up.abort();
    Ok(())
}
