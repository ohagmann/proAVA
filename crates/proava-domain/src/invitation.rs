//! Bieterverzeichnis: the bidders asked for a bid on one LV, and what came of it.

use crate::bid::{Address, BidId};

/// Id of an invited bidder, unique within its LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct InviteeId(pub u64);

/// Where an invitation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InviteStatus {
    /// Unterlagen angefordert / versandt, noch keine Antwort.
    #[default]
    Requested,
    /// Angebot abgegeben.
    Submitted,
    /// Kein Angebot (Absage oder nichts eingegangen).
    NoBid,
    /// Ausgeschlossen (z. B. verspätet, unvollständig).
    Excluded,
}

impl InviteStatus {
    pub const ALL: [InviteStatus; 4] = [
        InviteStatus::Requested,
        InviteStatus::Submitted,
        InviteStatus::NoBid,
        InviteStatus::Excluded,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InviteStatus::Requested => "angefragt",
            InviteStatus::Submitted => "Angebot abgegeben",
            InviteStatus::NoBid => "kein Angebot",
            InviteStatus::Excluded => "ausgeschlossen",
        }
    }

    /// Key in the project file and the MCP server.
    pub fn key(self) -> &'static str {
        match self {
            InviteStatus::Requested => "requested",
            InviteStatus::Submitted => "submitted",
            InviteStatus::NoBid => "no-bid",
            InviteStatus::Excluded => "excluded",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.key() == key)
    }
}

/// One bidder on the list of an LV (Bieterverzeichnis).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InvitedBidder {
    pub id: InviteeId,
    /// Firm with address; phone and e-mail of the firm.
    pub address: Address,
    /// Ansprechpartner.
    pub contact_person: String,
    /// Date the documents were requested/sent, TT.MM.JJJJ.
    pub requested: String,
    pub status: InviteStatus,
    /// The bid read in for this bidder (Angebot eingelesen).
    pub bid: Option<BidId>,
    /// Eingang des Angebots, e.g. „14.12.2026, 09:30 Uhr“.
    pub received: String,
    /// Nebenangebote abgegeben.
    pub side_offers: bool,
    /// Bemerkungen (für die Niederschrift).
    pub remark: String,
    /// Grund der Nichtberücksichtigung (Absageschreiben, Platzhalter `{absagegrund}`).
    pub rejection_reason: String,
}

impl InvitedBidder {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            ..Self::default()
        }
    }

    pub fn name(&self) -> &str {
        self.address.name1.trim()
    }
}

/// How well two firm names match: 0 = not at all, 100 = the same.
/// Ignores case, punctuation and legal forms (GmbH, KG, AG …).
pub fn name_similarity(left: &str, right: &str) -> u32 {
    let a = words(left);
    let b = words(right);
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    if a == b {
        return 100;
    }
    let joined_a = a.join("");
    let joined_b = b.join("");
    if joined_a == joined_b {
        return 95;
    }
    let common = a.iter().filter(|word| b.contains(word)).count();
    let score = (common * 100 / a.len().max(b.len())) as u32;
    if score == 0 && (joined_a.contains(&joined_b) || joined_b.contains(&joined_a)) {
        return 60;
    }
    score
}

const LEGAL_FORMS: [&str; 14] = [
    "gmbh",
    "mbh",
    "co",
    "kg",
    "ag",
    "ohg",
    "gbr",
    "ug",
    "e",
    "k",
    "ek",
    "haftungsbeschränkt",
    "und",
    "&",
];

fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|character: char| !character.is_alphanumeric() && character != '&')
        .filter(|word| !word.is_empty() && !LEGAL_FORMS.contains(word))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{name_similarity, InviteStatus};

    #[test]
    fn similar_names_match() {
        assert_eq!(
            name_similarity("Müller Elektro GmbH", "müller elektro"),
            100
        );
        assert_eq!(
            name_similarity("Müller-Elektro GmbH & Co. KG", "Müllerelektro GmbH"),
            95
        );
        assert!(name_similarity("Müller Elektro GmbH", "Elektro Müller und Söhne") >= 60);
        assert!(name_similarity("Schmidt Regeltechnik", "Müller Elektro") == 0);
        assert_eq!(name_similarity("", "x"), 0);
        for status in InviteStatus::ALL {
            assert_eq!(InviteStatus::from_key(status.key()), Some(status));
        }
    }
}
