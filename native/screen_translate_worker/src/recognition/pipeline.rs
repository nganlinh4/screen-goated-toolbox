use super::{
    local_support, prefer_reading,
    reader::Reader,
    router::{Route, Router},
};
use anyhow::{Result, bail};
use image::RgbImage;
use sgt_screen_text_detector_protocol::{
    recognition::{Completion, Reading},
    stream::Region,
};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};

pub(super) struct Capture<'a> {
    pub capture_id: u64,
    pub image: &'a RgbImage,
    pub regions: &'a [Region],
    pub cancel: &'a AtomicBool,
}

pub(super) fn recognize(
    routers: &mut [Router; 2],
    readers: &mut [(String, Reader)],
    alphabet: &HashSet<char>,
    capture: Capture<'_>,
    emit: &mut impl FnMut(&Completion) -> Result<()>,
) -> Result<()> {
    let Capture {
        capture_id,
        image,
        regions,
        cancel,
    } = capture;
    let started = Instant::now();
    let mut pixels = 0_u64;
    let mut crops = Vec::with_capacity(regions.len());
    for region in regions {
        check(cancel)?;
        let crop = crate::localization::crop(image, region)?;
        pixels += u64::from(crop.width()) * u64::from(crop.height());
        if pixels > 80_000_000 {
            bail!("reader crop set exceeds memory budget");
        }
        crops.push(crop);
    }
    // Independent sessions bound CPU work without changing recognition batch
    // membership or its padding geometry.
    let next = AtomicUsize::new(0);
    let routes = std::thread::scope(|scope| -> Result<Vec<Route>> {
        let (left, right) = routers.split_at_mut(1);
        let first = scope.spawn(|| classify(&mut left[0], &crops, &next, cancel));
        let second = classify(&mut right[0], &crops, &next, cancel);
        let mut routes = first
            .join()
            .map_err(|_| anyhow::anyhow!("script classifier thread failed"))??;
        routes.extend(second?);
        routes.sort_unstable_by_key(|(index, _)| *index);
        Ok(routes.into_iter().map(|(_, route)| route).collect())
    })?;
    eprintln!(
        "[DetectorPerf] capture={capture_id} routing_ms={:.1} regions={}",
        started.elapsed().as_secs_f64() * 1000.0,
        regions.len()
    );
    let primary_routes = routes.iter().map(|r| r.readers[0]).collect::<Vec<_>>();
    let ids = regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id, i))
        .collect::<std::collections::HashMap<_, _>>();
    let mut groups = vec![Vec::new(); readers.len()];
    let mut retry_groups = vec![Vec::new(); readers.len()];
    let mut retries = vec![None; regions.len()];
    for (i, route) in routes.iter().enumerate() {
        groups[route.readers[0]].push(i);
    }
    for ((name, reader), indices) in readers.iter_mut().zip(groups) {
        if indices.is_empty() {
            continue;
        }
        let selected = indices.iter().map(|&i| &crops[i]).collect::<Vec<_>>();
        let selected_regions = indices
            .iter()
            .map(|&i| regions[i].clone())
            .collect::<Vec<_>>();
        let started = Instant::now();
        reader.recognize(
            capture_id,
            &selected,
            &selected_regions,
            cancel,
            |completion| {
                let i = ids[&completion.region_id];
                let [primary, alternative] = routes[i].readers;
                let retry = should_retry(routes[i], &completion.reading,
                    local_support(i, alternative, &primary_routes, regions));
                if retry || matches!(completion.reading, Reading::Unresolved(_)) {
                    eprintln!("[DetectorPerf] capture={capture_id} region={} primary={primary} alternative={alternative} retry={retry} unreadable={}",
                        completion.region_id, matches!(completion.reading, Reading::Unresolved(_)));
                }
                if retry {
                    retries[i] = Some(completion.clone());
                    retry_groups[alternative].push(i);
                    Ok(())
                } else {
                    emit(completion)
                }
            },
        )?;
        eprintln!(
            "[DetectorPerf] capture={capture_id} reader={name} attempt=0 regions={} reader_ms={:.1}",
            indices.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    for ((name, reader), indices) in readers.iter_mut().zip(retry_groups) {
        if indices.is_empty() {
            continue;
        }
        let selected = indices.iter().map(|&i| &crops[i]).collect::<Vec<_>>();
        let selected_regions = indices
            .iter()
            .map(|&i| regions[i].clone())
            .collect::<Vec<_>>();
        let started = Instant::now();
        reader.recognize(
            capture_id,
            &selected,
            &selected_regions,
            cancel,
            |completion| {
                let primary = retries[ids[&completion.region_id]]
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("missing primary reading"))?;
                emit(&prefer_reading(primary, completion.clone(), alphabet))
            },
        )?;
        eprintln!(
            "[DetectorPerf] capture={capture_id} reader={name} attempt=1 regions={} reader_ms={:.1}",
            indices.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}

fn classify(
    router: &mut Router,
    crops: &[RgbImage],
    next: &AtomicUsize,
    cancel: &AtomicBool,
) -> Result<Vec<(usize, Route)>> {
    let mut routes = Vec::new();
    loop {
        check(cancel)?;
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(crop) = crops.get(index) else {
            break;
        };
        routes.push((index, router.select(crop)?));
    }
    Ok(routes)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        bail!("capture cancelled");
    }
    Ok(())
}

fn should_retry(route: Route, reading: &Reading, neighbor_support: bool) -> bool {
    let [primary, alternative] = route.readers;
    primary != alternative
        && if primary == 0 {
            neighbor_support
                || (route.alternative_observed
                    && match reading {
                        Reading::Unresolved(_) => true,
                        Reading::Text(text) => !text.chars().any(char::is_alphanumeric),
                    })
        } else {
            matches!(reading, Reading::Unresolved(_))
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_requires_evidence_and_preserves_numbers_and_words() {
        let weak = Route {
            readers: [0, 1],
            alternative_observed: false,
        };
        let supported = Route {
            alternative_observed: true,
            ..weak
        };
        assert!(!should_retry(weak, &Reading::Text(".".into()), false));
        assert!(!should_retry(
            weak,
            &Reading::Unresolved(String::new()),
            false
        ));
        assert!(should_retry(supported, &Reading::Text(".".into()), false));
        assert!(should_retry(
            supported,
            &Reading::Unresolved(String::new()),
            false
        ));
        for text in ["123", "3.14", "word", "文字"] {
            assert!(!should_retry(supported, &Reading::Text(text.into()), false));
        }
        assert!(should_retry(weak, &Reading::Text("word".into()), true));
    }
}
