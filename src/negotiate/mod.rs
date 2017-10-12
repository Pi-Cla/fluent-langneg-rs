//! Language Negotiation is a process in which locales from different
//! sources are filtered and sorted in an effort to produce the best
//! possible selection of them.
//!
//! There are multiple language negotiation strategies, most popular is
//! described in [RFC4647](https://www.ietf.org/rfc/rfc4647.txt).
//!
//! The algorithm is based on the BCP4647 3.3.2 Extended Filtering algorithm,
//! with several modifications.
//!
//! # Example:
//!
//! ```
//! use fluent_locale::negotiate_languages;
//! use fluent_locale::NegotiationStrategy;
//!
//! let supported = negotiate_languages(
//!   &["pl", "fr", "en-US"],                    // requested
//!   &["it", "de", "fr", "en-GB", "en-US"],     // available
//!   Some("en-US"),                             // default
//!   &NegotiationStrategy::Filtering            // strategy
//! );
//! assert_eq!(supported, vec!["fr", "en-US", "en-GB"]);
//! ```
//!
//! # The exact algorithm is custom, and consists of a 6 level strategy:
//!
//! ### 1) Attempt to find an exact match for each requested locale in available locales.
//!
//! Example:
//!
//! ```text
//! // [requested] * [available] = [supported]
//!
//! ["en-US"] * ["en-US"] = ["en-US"]
//! ```
//!
//! ### 2) Attempt to match a requested locale to an available locale treated as a locale range.
//!
//! Example:
//!
//! ```text
//! // [requested] * [available] = [supported]
//!
//! ["en-US"] * ["en"] = ["en"]
//!               ^^
//!                |-- becomes "en-*-*-*"
//! ```
//!
//! ### 3) Maximize the requested locale to find the best match in available locales.
//!
//! This part uses ICU's likelySubtags or similar database.
//!
//! Example:
//!
//! ```text
//! // [requested] * [available] = [supported]
//!
//! ["en"] * ["en-GB", "en-US"] = ["en-US"]
//!   ^^       ^^^^^    ^^^^^
//!    |           |        |
//!    |           |----------- become "en-*-GB-*" and "en-*-US-*"
//!    |
//!    |-- ICU likelySubtags expands it to "en-Latn-US"
//! ```
//!
//! ### 4) Attempt to look up for a different variant of the same locale.
//!
//! Example:
//!
//! ```text
//! // [requested] * [available] = [supported]
//!
//! ["ja-JP-win"] * ["ja-JP-mac"] = ["ja-JP-mac"]
//!   ^^^^^^^^^       ^^^^^^^^^
//!           |               |-- become "ja-*-JP-mac"
//!           |
//!           |----------- replace variant with range: "ja-JP-*"
//! ```
//!
//! ### 5) Look up for a maximized version of the requested locale, stripped of the region code.
//!
//! Example:
//!
//! ```text
//! // [requested] * [available] = [supported]
//!
//! ["en-CA"] * ["en-ZA", "en-US"] = ["en-US", "en-ZA"]
//!   ^^^^^
//!       |       ^^^^^    ^^^^^
//!       |           |        |
//!       |           |----------- become "en-*-ZA-*" and "en-*-US-*"
//!       |
//!       |----------- strip region produces "en", then lookup likelySubtag: "en-Latn-US"
//! ```
//!
//!
//! ### 6) Attempt to look up for a different region of the same locale.
//!
//! Example:
//!
//! ```text
//! // [requested] * [available] = [supported]
//!
//! ["en-GB"] * ["en-AU"] = ["en-AU"]
//!   ^^^^^       ^^^^^
//!       |           |-- become "en-*-AU-*"
//!       |
//!       |----- replace region with range: "en-*"
//! ```
//!

use std::collections::HashMap;
use super::locale::Locale;

mod likely_subtags;

#[derive(PartialEq, Debug)]
pub enum NegotiationStrategy {
    Filtering,
    Matching,
    Lookup,
}

fn filter_matches<'a>(
    requested: &[&'a str],
    available: &[&'a str],
    strategy: &NegotiationStrategy,
) -> Vec<&'a str> {

    let mut available_locales: HashMap<&str, Locale> = HashMap::new();
    let mut available = available.to_vec();

    available.retain(|tag| match Locale::new(tag, None) {
        Ok(loc) => {
            available_locales.insert(tag, loc);
            true
        }
        Err(_) => false,
    });

    let mut supported_locales = vec![];

    for req_loc_str in requested {
        if req_loc_str.is_empty() {
            continue;
        }

        let mut requested_locale = Locale::from(*req_loc_str);

        let mut match_found = false;

        // 1) Try to find a simple (case-insensitive) string match for the request.
        available.retain(|key| {
            if strategy != &NegotiationStrategy::Filtering && match_found {
                return true;
            }

            if available_locales
                .get(key)
                .expect("Available key should be available")
                .matches(&requested_locale, false, false)
            {
                supported_locales.push(*key);
                match_found = true;
                return false;
            }
            true
        });

        if match_found {
            match *strategy {
                NegotiationStrategy::Filtering => {}
                NegotiationStrategy::Matching => continue,
                NegotiationStrategy::Lookup => break,
            }
        }

        // 2) Try to match against the available locales treated as ranges.
        available.retain(|key| {
            if strategy != &NegotiationStrategy::Filtering && match_found {
                return true;
            }

            if available_locales
                .get(key)
                .expect("Available key should be available")
                .matches(&requested_locale, true, false)
            {
                supported_locales.push(*key);
                match_found = true;
                return false;
            }
            true
        });

        if match_found {
            match *strategy {
                NegotiationStrategy::Filtering => {}
                NegotiationStrategy::Matching => continue,
                NegotiationStrategy::Lookup => break,
            };
        }

        match_found = false;

        // 3) Try to match against a maximized version of the requested locale
        if let Some(extended) = likely_subtags::add(requested_locale.to_string().as_ref()) {
            requested_locale = Locale::from(extended);
            available.retain(|key| {
                if strategy != &NegotiationStrategy::Filtering && match_found {
                    return true;
                }

                if available_locales
                    .get(key)
                    .expect("Available key should be available")
                    .matches(&requested_locale, true, false)
                {
                    supported_locales.push(*key);
                    match_found = true;
                    return false;
                }
                true
            });
        }

        if match_found {
            match *strategy {
                NegotiationStrategy::Filtering => {}
                NegotiationStrategy::Matching => continue,
                NegotiationStrategy::Lookup => break,
            };
        }

        match_found = false;

        // 4) Try to match against a variant as a range
        requested_locale.clear_variants();
        available.retain(|key| {
            if strategy != &NegotiationStrategy::Filtering && match_found {
                return true;
            }

            if available_locales
                .get(key)
                .expect("Available key should be available")
                .matches(&requested_locale, true, true)
            {
                supported_locales.push(*key);
                match_found = true;
                return false;
            }
            true
        });

        if match_found {
            match *strategy {
                NegotiationStrategy::Filtering => {}
                NegotiationStrategy::Matching => continue,
                NegotiationStrategy::Lookup => break,
            };
        }

        match_found = false;

        // 5) Try to match against the likely subtag without region
        requested_locale.set_region("").unwrap();
        if let Some(extended) = likely_subtags::add(requested_locale.to_string().as_ref()) {
            let requested_locale = Locale::from(extended);
            available.retain(|key| {
                if strategy != &NegotiationStrategy::Filtering && match_found {
                    return true;
                }

                if available_locales
                    .get(key)
                    .expect("Available key should be available")
                    .matches(&requested_locale, true, false)
                {
                    supported_locales.push(*key);
                    match_found = true;
                    return false;
                }
                true
            });
        }

        if match_found {
            match *strategy {
                NegotiationStrategy::Filtering => {}
                NegotiationStrategy::Matching => continue,
                NegotiationStrategy::Lookup => break,
            };
        }

        match_found = false;

        // 6) Try to match against a region as a range
        requested_locale.set_region("").unwrap();
        available.retain(|key| {
            if strategy != &NegotiationStrategy::Filtering && match_found {
                return true;
            }

            if available_locales
                .get(key)
                .expect("Available key should be available")
                .matches(&requested_locale, true, true)
            {
                supported_locales.push(*key);
                match_found = true;
                return false;
            }
            true
        });

        if match_found {
            match *strategy {
                NegotiationStrategy::Filtering => {}
                NegotiationStrategy::Matching => continue,
                NegotiationStrategy::Lookup => break,
            };
        }
    }

    supported_locales
}

pub fn negotiate_languages<'a>(
    requested: &[&'a str],
    available: &[&'a str],
    default: Option<&'a str>,
    strategy: &NegotiationStrategy,
) -> Vec<&'a str> {
    let mut supported = filter_matches(requested, available, strategy);

    if let Some(d) = default {
        if strategy == &NegotiationStrategy::Lookup {
            if supported.is_empty() {
                supported.push(d);
            }
        } else if !supported.contains(&d) {
            supported.push(d);
        }
    }
    supported
}
