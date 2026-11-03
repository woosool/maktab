use crate::model::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};
struct Pop {
    stream: BufReader<native_tls::TlsStream<TcpStream>>,
}
impl Pop {
    fn line(&mut self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        let n = self
            .stream
            .by_ref()
            .take(64 * 1024)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if n == 0 || !bytes.ends_with(b"\n") {
            return Err("POP server closed the connection or returned an oversized line".into());
        }
        Ok(bytes)
    }
    fn ok(&mut self) -> Result<(), String> {
        if self.line()?.starts_with(b"+OK") {
            Ok(())
        } else {
            Err(
                "POP server rejected the request. Check your credentials and server settings."
                    .into(),
            )
        }
    }
    fn command(&mut self, command: &str) -> Result<(), String> {
        if command.contains(['\r', '\n']) {
            return Err("Invalid POP command".into());
        }
        self.stream
            .get_mut()
            .write_all(format!("{command}\r\n").as_bytes())
            .map_err(|e| e.to_string())?;
        self.ok()
    }
    fn multiline(&mut self, command: &str) -> Result<Vec<u8>, String> {
        self.command(command)?;
        let mut result = Vec::new();
        loop {
            let line = self.line()?;
            if line == b".\r\n" || line == b".\n" {
                break;
            }
            result.extend_from_slice(if line.starts_with(b"..") {
                &line[1..]
            } else {
                &line
            });
            if result.len() > 50 * 1024 * 1024 {
                return Err("POP message exceeds the 50 MB download limit".into());
            }
        }
        Ok(result)
    }
}
use std::io::Read;
fn connect(a: &Account, secret: &str) -> Result<Pop, String> {
    if a.email.contains(['\r', '\n']) || secret.contains(['\r', '\n']) {
        return Err("Invalid login characters".into());
    }
    let addr = (a.imap_host.as_str(), a.imap_port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("POP host has no address")?;
    let stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(20)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let tls = native_tls::TlsConnector::new()
        .map_err(|e| e.to_string())?
        .connect(&a.imap_host, stream)
        .map_err(|e| e.to_string())?;
    let mut pop = Pop {
        stream: BufReader::new(tls),
    };
    pop.ok()?;
    pop.command(&format!("USER {}", a.email))?;
    pop.command(&format!("PASS {secret}"))?;
    Ok(pop)
}
pub fn test(a: &Account, secret: &str) -> Result<(), String> {
    let mut pop = connect(a, secret)?;
    pop.command("QUIT")
}
pub fn sync(a: &Account) -> Result<SyncBatch, String> {
    let mut pop = connect(a, &super::password(a)?)?;
    let uidls = pop.multiline("UIDL")?;
    let uidls = String::from_utf8(uidls).map_err(|_| "Invalid POP UIDL response")?;
    let mut messages = Vec::new();
    for line in uidls.lines() {
        let (index, id) = parse_uidl(line)?;
        let raw = pop.multiline(&format!("RETR {index}"))?;
        messages.push(super::parse_remote(a, &id, "Inbox", &raw)?);
    }
    pop.command("QUIT")?;
    Ok(SyncBatch {
        failed_folders: Vec::new(),
        warnings: Vec::new(),
        messages,
        folders: ["Inbox", "Sent", "Drafts", "Archive", "Junk", "Trash"]
            .into_iter()
            .map(|name| Folder {
                is_label: false,
                account: a.id.clone(),
                path: name.into(),
                name: name.into(),
            })
            .collect(),
    })
}
fn parse_uidl(line: &str) -> Result<(u32, String), String> {
    let mut parts = line.split_whitespace();
    let index = parts
        .next()
        .ok_or("Missing POP message number")?
        .parse::<u32>()
        .map_err(|_| "Invalid POP message number")?;
    let id = parts
        .next()
        .filter(|s| s.len() <= 70 && s.bytes().all(|b| (0x21..=0x7e).contains(&b)))
        .ok_or("Invalid POP UIDL")?;
    if index == 0 || parts.next().is_some() {
        return Err("Invalid POP UIDL line".into());
    }
    Ok((index, id.into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_uidls_are_validated_before_retrieval() {
        assert_eq!(parse_uidl("1 stable-id").unwrap(), (1, "stable-id".into()));
        assert!(parse_uidl("0 bad").is_err());
        assert!(parse_uidl("1 bad extra").is_err());
        assert!(parse_uidl("RETR injected").is_err());
    }
}
