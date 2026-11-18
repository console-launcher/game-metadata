// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A picked folder as a Wii U title: the folder that holds `meta/meta.xml`.

use gamemeta_core::ByteSource;
use gamemeta_core::wiiu::TitleFiles;
use wasm_bindgen::JsValue;

use crate::source::BlobSource;

const META: &str = "meta/meta.xml";

/// The folder (with a trailing `/`, empty for the top) that holds `meta/meta.xml`, the shallowest one; `None` when no path ends
/// in it. Paths are relative with `/` separators, as `webkitRelativePath` gives them.
pub fn title_root(paths: &[String]) -> Option<String> {
    paths
        .iter()
        .filter_map(|p| p.strip_suffix(META).filter(|root| root.is_empty() || root.ends_with('/')))
        .min_by_key(|root| root.matches('/').count())
        .map(str::to_string)
}

/// The files of one title folder, found by path below its root.
#[derive(Debug)]
pub struct FolderFiles {
    root: String,
    files: Vec<(String, JsValue)>,
}

impl FolderFiles {
    pub fn new(paths: Vec<String>, blobs: Vec<JsValue>) -> Option<FolderFiles> {
        let root = title_root(&paths)?;
        Some(FolderFiles { root, files: paths.into_iter().zip(blobs).collect() })
    }

    fn find(&self, path: &str) -> Option<BlobSource> {
        let want = format!("{}{path}", self.root);
        self.files.iter().find(|(p, _)| *p == want).map(|(_, blob)| BlobSource::new(blob.clone()))
    }
}

impl TitleFiles for FolderFiles {
    fn size(&mut self, path: &str) -> Option<u64> {
        Some(self.find(path)?.size())
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let mut source = self.find(path)?;
        let size = usize::try_from(source.size()).ok().filter(|&n| n <= max)?;
        let mut bytes = vec![0u8; size];
        source.read_at(0, &mut bytes).ok()?;
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::title_root;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_root_is_the_folder_around_meta_xml() {
        let p = paths(&["Game/code/a.rpx", "Game/meta/meta.xml", "Game/meta/iconTex.tga", "Game/content/x.bin"]);
        assert_eq!(title_root(&p).as_deref(), Some("Game/"));
    }

    #[test]
    fn a_meta_xml_at_the_top_has_an_empty_root() {
        assert_eq!(title_root(&paths(&["meta/meta.xml"])).as_deref(), Some(""));
    }

    #[test]
    fn the_shallowest_title_wins_and_lookalikes_do_not_count() {
        let p = paths(&["Dir/Deep/Game/meta/meta.xml", "Dir/Game2/meta/meta.xml", "Dir/notmeta/meta.xml"]);
        assert_eq!(title_root(&p).as_deref(), Some("Dir/Game2/"));
        assert_eq!(title_root(&paths(&["Dir/xmeta/meta.xml", "readme.txt"])), None);
        assert_eq!(title_root(&[]), None);
    }
}
