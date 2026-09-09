use super::catalog::{self, Catalog};
use super::router_preprocess::preprocess;
use anyhow::{Context, Result, bail};
use image::RgbImage;
use ort::{session::Session, value::Tensor};
use std::path::Path;

pub(super) struct Router {
    session: Session,
    routes: Vec<usize>,
    blank: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Route {
    pub readers: [usize; 2],
    pub alternative_observed: bool,
}

impl Router {
    pub fn load(root: &Path, catalog: &Catalog) -> Result<Self> {
        let labels: Vec<String> =
            serde_json::from_slice(&std::fs::read(catalog::file(root, &catalog.labels)?)?)?;
        if labels.is_empty() || labels.len() > 256 {
            bail!("invalid script labels");
        }
        let blank = labels
            .iter()
            .position(|label| label == "Broken")
            .context("missing script blank label")?;
        let routes = labels
            .iter()
            .map(|label| {
                let script = label.strip_suffix("-dn").unwrap_or(label);
                let script = script.strip_suffix("_vert").unwrap_or(script);
                catalog
                    .readers
                    .iter()
                    .position(|reader| reader.scripts.iter().any(|s| s == script))
                    .unwrap_or(0)
            })
            .collect();
        let session = Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_inter_threads(1)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .commit_from_file(catalog::file(root, &catalog.router)?)?;
        let mut router = Self {
            session,
            routes,
            blank,
        };
        router.select(&RgbImage::from_pixel(320, 48, image::Rgb([255, 255, 255])))?;
        Ok(router)
    }

    pub fn select(&mut self, crop: &RgbImage) -> Result<Route> {
        let (width, input) = preprocess(crop)?;
        let tensor = Tensor::from_array((vec![1_usize, 1, 48, width], input))?;
        let output = self.session.run(ort::inputs![tensor])?;
        let (shape, scores) = output[0].try_extract_tensor::<f32>()?;
        if shape.len() != 2 || shape[0] <= 0 || shape[1] != self.routes.len() as i64 {
            bail!("invalid script classifier output");
        }
        let mut counts = vec![0; self.routes.len()];
        let mut support = vec![0.0_f32; self.routes.iter().max().copied().unwrap_or(0) + 1];
        let mut previous = self.blank;
        for row in scores.chunks_exact(self.routes.len()) {
            if row.iter().any(|p| !p.is_finite()) {
                bail!("nonfinite script classifier output");
            }
            for (&route, &probability) in self.routes.iter().zip(row) {
                if route != 0 {
                    support[route] += probability;
                }
            }
            let index = row
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .context("empty script output")?
                .0;
            if index != self.blank && index != previous {
                counts[index] += 1;
            }
            previous = index;
        }
        // Unknown/common labels retain the general reader. They never discard
        // a region or activate an open-ended recognizer search.
        let primary = counts
            .iter()
            .enumerate()
            .max_by_key(|&(i, count)| (*count, std::cmp::Reverse(i)))
            .map(|(index, _)| self.routes[index])
            .unwrap_or(0);
        let alternative = if primary != 0 {
            0
        } else {
            support
                .iter()
                .enumerate()
                .skip(1)
                .max_by(|a, b| a.1.total_cmp(b.1))
                .filter(|(_, score)| **score > 0.0)
                .map(|(index, _)| index)
                .unwrap_or(0)
        };
        let alternative_votes = counts
            .iter()
            .zip(&self.routes)
            .filter(|(_, route)| **route == alternative)
            .map(|(count, _)| *count)
            .sum::<usize>();
        Ok(Route {
            readers: [primary, alternative],
            alternative_observed: alternative_votes >= 2,
        })
    }
}
