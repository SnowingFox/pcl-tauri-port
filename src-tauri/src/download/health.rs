//! Which download hosts are misbehaving right now, and which candidate URL to try next.
//!
//! The same file is usually reachable from two origins (the official CDN and a mirror) and which
//! one is fast depends on the user's network and the hour. Instead of always trying them in a fixed
//! order, a host that just failed or crawled is moved to the back of the queue for a while, and a
//! file alternates between its candidates instead of spending every retry on the same one.

use super::error::DlError;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long a host that just failed or crawled is tried last.
pub const PENALTY: Duration = Duration::from_secs(60);

fn board() -> &'static Mutex<HashMap<String, Instant>> {
    static B: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

/// `host:port` of a URL (empty when it has none): the unit health is tracked per.
pub fn host_key(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            let h = u.host_str()?.to_string();
            Some(match u.port_or_known_default() {
                Some(p) => format!("{h}:{p}"),
                None => h,
            })
        })
        .unwrap_or_default()
}

pub fn penalize(url: &str) {
    penalize_for(url, PENALTY);
}

pub fn penalize_for(url: &str, d: Duration) {
    let now = Instant::now();
    let mut b = board().lock().unwrap();
    b.retain(|_, until| *until > now);
    b.insert(host_key(url), now + d);
}

pub fn is_penalized(url: &str) -> bool {
    board().lock().unwrap().get(&host_key(url)).is_some_and(|until| *until > Instant::now())
}

/// A request to this host just worked: it is healthy again.
pub fn succeeded(url: &str) {
    forget(url);
}

pub fn forget(url: &str) {
    board().lock().unwrap().remove(&host_key(url));
}

/// The candidate URLs for one file and how many attempts each has used.
///
/// The next URL to try is the one that has been tried least, then the one whose host is not
/// penalized, then the first in the original order. So a file alternates between its candidates, a
/// host that is currently in trouble is tried last, and ties keep the caller's preference
/// (official first, or mirror first).
pub struct Plan {
    urls: Vec<String>,
    tries: Vec<u32>,
    dead: Vec<bool>,
    max_tries: u32,
}

impl Plan {
    pub fn new(urls: Vec<String>, max_tries: u32) -> Self {
        let n = urls.len();
        Self { urls, tries: vec![0; n], dead: vec![false; n], max_tries: max_tries.max(1) }
    }

    pub fn url(&self, i: usize) -> &str {
        &self.urls[i]
    }

    pub fn tries(&self, i: usize) -> u32 {
        self.tries[i]
    }

    fn usable(&self, i: usize) -> bool {
        !self.dead[i] && self.tries[i] < self.max_tries
    }

    /// Index of the URL to try next, or `None` when every candidate is used up.
    pub fn next(&self) -> Option<usize> {
        (0..self.urls.len()).filter(|&i| self.usable(i)).min_by_key(|&i| (self.tries[i], is_penalized(&self.urls[i]), i))
    }

    /// Is there another origin, not currently in trouble, that could take over from `i`?
    /// Only then is it worth giving up on a slow connection.
    pub fn has_alternative(&self, i: usize) -> bool {
        let host = host_key(&self.urls[i]);
        (0..self.urls.len()).any(|j| j != i && self.usable(j) && !is_penalized(&self.urls[j]) && host_key(&self.urls[j]) != host)
    }

    /// Record a failed attempt on `i`. A permanent error (the file is not there) retires the
    /// candidate without blaming its host; a network-ish error penalizes the host.
    pub fn failed(&mut self, i: usize, err: &DlError) {
        self.tries[i] += 1;
        if err.is_permanent() {
            self.dead[i] = true;
        } else if err.blames_host() {
            penalize(&self.urls[i]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(base: &str) -> Vec<String> {
        vec![format!("http://{base}-a.test:81/f"), format!("http://{base}-b.test:82/f")]
    }

    fn net_err() -> DlError {
        DlError::Status(503, "u".into())
    }

    #[test]
    fn a_file_alternates_between_candidates_instead_of_repeating_one() {
        let u = urls("alt");
        u.iter().for_each(|x| forget(x));
        let mut p = Plan::new(u, 3);
        let mut order = vec![];
        while let Some(i) = p.next() {
            order.push(i);
            p.failed(i, &net_err());
        }
        assert_eq!(order, [0, 1, 0, 1, 0, 1], "6 attempts, alternating, official first");
    }

    #[test]
    fn a_penalized_host_is_tried_last_and_success_forgives_it() {
        let u = urls("pen");
        u.iter().for_each(|x| forget(x));
        penalize(&u[0]);
        let p = Plan::new(u.clone(), 3);
        assert_eq!(p.next(), Some(1), "the healthy candidate goes first although it is second");
        succeeded(&u[0]);
        assert_eq!(Plan::new(u.clone(), 3).next(), Some(0), "forgiven: the caller's order applies again");
        u.iter().for_each(|x| forget(x));
    }

    #[test]
    fn a_penalty_expires() {
        let u = urls("exp");
        u.iter().for_each(|x| forget(x));
        penalize_for(&u[0], Duration::from_millis(40));
        assert!(is_penalized(&u[0]));
        std::thread::sleep(Duration::from_millis(80));
        assert!(!is_penalized(&u[0]));
        assert_eq!(Plan::new(u, 3).next(), Some(0));
    }

    #[test]
    fn a_missing_file_retires_the_candidate_without_penalizing_its_host() {
        let u = urls("perm");
        u.iter().for_each(|x| forget(x));
        let mut p = Plan::new(u.clone(), 3);
        p.failed(0, &DlError::Status(404, "u".into()));
        assert!(!is_penalized(&u[0]), "a 404 says the file is missing, not that the host is unhealthy");
        assert_eq!(p.next(), Some(1));
        p.failed(1, &net_err());
        assert_eq!(p.next(), Some(1), "the retired candidate is never picked again");
        assert!(is_penalized(&u[1]));
        u.iter().for_each(|x| forget(x));
    }

    #[test]
    fn local_errors_do_not_blame_the_host() {
        let u = urls("local");
        u.iter().for_each(|x| forget(x));
        let mut p = Plan::new(u.clone(), 3);
        p.failed(0, &DlError::Io(std::io::Error::from_raw_os_error(24))); // too many open files
        assert!(!is_penalized(&u[0]));
        assert_eq!(p.tries(0), 1, "it still used up an attempt");
    }

    #[test]
    fn an_alternative_must_be_another_healthy_origin() {
        let u = urls("alt2");
        u.iter().for_each(|x| forget(x));
        let p = Plan::new(u.clone(), 3);
        assert!(p.has_alternative(0));
        penalize(&u[1]);
        assert!(!p.has_alternative(0), "the other origin is in trouble too");
        forget(&u[1]);
        let same_host = Plan::new(vec!["http://same.test:80/a".into(), "http://same.test:80/b".into()], 3);
        assert!(!same_host.has_alternative(0), "a second URL on the same host is not another origin");
        u.iter().for_each(|x| forget(x));
    }
}
