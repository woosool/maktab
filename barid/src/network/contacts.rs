use super::oauth::{access, client, response};
use crate::model::*;
use serde_json::Value;
pub fn sync(a: &Account) -> Result<Vec<Contact>, String> {
    match a.connection {
        Connection::Gmail => google(a),
        Connection::Outlook => outlook(a),
        _ => carddav(a),
    }
}
fn contact(
    a: &Account,
    id: &str,
    name: &str,
    email: &str,
    phone: &str,
    company: &str,
) -> Option<Contact> {
    if email.parse::<lettre::Address>().is_err() {
        return None;
    }
    Some(Contact {
        id: format!("{}:contact:{id}:{}", a.id, email.to_lowercase()),
        source_account: a.id.clone(),
        source_id: id.into(),
        name: name.into(),
        email: email.into(),
        phone: phone.into(),
        company: company.into(),
        ..Default::default()
    })
}
fn google(a: &Account) -> Result<Vec<Contact>, String> {
    let http = client()?;
    let token = access(a)?;
    let mut next = String::new();
    let mut contacts = Vec::new();
    loop {
        let mut url = url::Url::parse("https://people.googleapis.com/v1/people/me/connections")
            .map_err(|e| e.to_string())?;
        url.query_pairs_mut().extend_pairs([
            (
                "personFields",
                "names,emailAddresses,phoneNumbers,organizations",
            ),
            ("pageSize", "1000"),
        ]);
        if !next.is_empty() {
            url.query_pairs_mut().append_pair("pageToken", &next);
        }
        let page: Value = response(
            http.get(url)
                .bearer_auth(&token)
                .send()
                .map_err(|e| e.to_string())?,
        )?
        .json()
        .map_err(|e| e.to_string())?;
        for item in page["connections"].as_array().into_iter().flatten() {
            let id = item["resourceName"]
                .as_str()
                .ok_or("Contact has no resource ID")?;
            for email in item["emailAddresses"].as_array().into_iter().flatten() {
                if let Some(c) = contact(
                    a,
                    id,
                    item["names"][0]["displayName"].as_str().unwrap_or(""),
                    email["value"].as_str().unwrap_or(""),
                    item["phoneNumbers"][0]["value"].as_str().unwrap_or(""),
                    item["organizations"][0]["name"].as_str().unwrap_or(""),
                ) {
                    contacts.push(c);
                }
            }
        }
        next = page["nextPageToken"].as_str().unwrap_or("").into();
        if next.is_empty() {
            break;
        }
    }
    Ok(contacts)
}
fn outlook(a: &Account) -> Result<Vec<Contact>, String> {
    let http = client()?;
    let token = access(a)?;
    let mut next=Some("https://graph.microsoft.com/v1.0/me/contacts?$top=100&$select=id,displayName,emailAddresses,mobilePhone,companyName".into());
    let mut contacts = Vec::new();
    while let Some(url) = next {
        let page: Value = response(
            http.get(url)
                .bearer_auth(&token)
                .send()
                .map_err(|e| e.to_string())?,
        )?
        .json()
        .map_err(|e| e.to_string())?;
        for item in page["value"].as_array().into_iter().flatten() {
            for email in item["emailAddresses"].as_array().into_iter().flatten() {
                if let Some(c) = contact(
                    a,
                    item["id"].as_str().ok_or("Contact has no ID")?,
                    item["displayName"].as_str().unwrap_or(""),
                    email["address"].as_str().unwrap_or(""),
                    item["mobilePhone"].as_str().unwrap_or(""),
                    item["companyName"].as_str().unwrap_or(""),
                ) {
                    contacts.push(c);
                }
            }
        }
        next = super::api::next_graph(&page)?;
    }
    Ok(contacts)
}
fn carddav(a: &Account) -> Result<Vec<Contact>, String> {
    let url = url::Url::parse(&a.contacts_url)
        .map_err(|_| "Set this account's CardDAV address-book URL in Settings → Contacts sync.")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(
            "CardDAV requires an HTTPS address-book URL without embedded credentials.".into(),
        );
    }
    let body = r#"<?xml version="1.0"?><c:addressbook-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><d:getetag/><c:address-data/></d:prop><c:filter/></c:addressbook-query>"#;
    let xml = response(
        client()?
            .request(
                reqwest::Method::from_bytes(b"REPORT").map_err(|e| e.to_string())?,
                url,
            )
            .basic_auth(&a.email, Some(super::password(a)?))
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body)
            .send()
            .map_err(|e| e.to_string())?,
    )?
    .text()
    .map_err(|e| e.to_string())?;
    parse_carddav(a, &xml)
}
fn parse_carddav(a: &Account, xml: &str) -> Result<Vec<Contact>, String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(xml);
    let mut data = String::new();
    let mut inside = false;
    let mut contacts = Vec::new();
    let mut multistatus = false;
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => {
                if e.local_name().as_ref() == b"multistatus" {
                    multistatus = true;
                }
                if e.local_name().as_ref() == b"address-data" {
                    inside = true;
                    data.clear();
                }
            }
            Event::Text(e) if inside => data.push_str(
                &quick_xml::escape::unescape(&e.decode().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            ),
            Event::CData(e) if inside => data.push_str(&e.decode().map_err(|e| e.to_string())?),
            Event::GeneralRef(e) if inside => {
                let name = e.decode().map_err(|e| e.to_string())?;
                data.push_str(
                    &quick_xml::escape::unescape(&format!("&{name};"))
                        .map_err(|e| e.to_string())?,
                );
            }
            Event::End(e) if e.local_name().as_ref() == b"address-data" => {
                inside = false;
                contacts.extend(vcard(a, &data));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !multistatus {
        return Err(
            "The server did not return a CardDAV address book. Check the collection URL.".into(),
        );
    }
    Ok(contacts)
}
fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut escape = false;
    for c in value.chars() {
        if escape {
            out.push(if c == 'n' || c == 'N' { '\n' } else { c });
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else {
            out.push(c);
        }
    }
    out
}
fn vcard(a: &Account, raw: &str) -> Vec<Contact> {
    let unfolded = raw
        .replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "");
    let (mut id, mut name, mut phone, mut company) =
        (String::new(), String::new(), String::new(), String::new());
    let mut emails = Vec::new();
    for line in unfolded.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = unescape(value);
        match key.split(';').next().unwrap_or("").to_uppercase().as_str() {
            "UID" => id = value,
            "FN" => name = value,
            "TEL" => phone = value,
            "ORG" => company = value.replace(';', " "),
            "EMAIL" => emails.push(value),
            _ => {}
        }
    }
    emails
        .into_iter()
        .filter_map(|email| {
            contact(
                a,
                if id.is_empty() { &email } else { &id },
                &name,
                &email,
                &phone,
                &company,
            )
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn carddav_parses_namespaces_folding_and_multiple_emails() {
        let a = Account {
            id: "a".into(),
            ..Default::default()
        };
        let c=parse_carddav(&a,"<d:multistatus xmlns:d='DAV:' xmlns:c='urn:ietf:params:xml:ns:carddav'><c:address-data><![CDATA[BEGIN:VCARD\r\nUID:123\r\nFN:Amelia\r\n Chen\r\nEMAIL:one@example.com\r\nEMAIL:two@example.com\r\nEND:VCARD]]></c:address-data></d:multistatus>").unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "AmeliaChen");
        assert_ne!(c[0].id, c[1].id);
        assert_eq!(c[0].source_account, "a");
        assert!(parse_carddav(&a, "<html/>").is_err());
    }
}
