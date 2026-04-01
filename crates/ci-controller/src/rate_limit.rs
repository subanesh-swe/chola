use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{body::Body, extract::ConnectInfo, http::Request};

pub struct RateLimiter {
    state: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    max_requests: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            max_requests,
            window,
        }
    }

    /// Returns true if the request is within the rate limit.
    pub fn check(&self, ip: IpAddr) -> bool {
        let mut map = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let entry = map.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (1, now);
            true
        } else if entry.0 < self.max_requests {
            entry.0 += 1;
            true
        } else {
            false
        }
    }
}

/// Extract client IP from ConnectInfo or X-Forwarded-For, falling back to UNSPECIFIED.
pub fn extract_ip(req: &Request<Body>) -> IpAddr {
    if let Some(ci) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return ci.0.ip();
    }
    if let Some(fwd) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = fwd.to_str() {
            if let Ok(ip) = s.split(',').next().unwrap_or("").trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }
    IpAddr::V4(Ipv4Addr::UNSPECIFIED)
}
