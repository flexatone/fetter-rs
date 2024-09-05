mod dep_manifest;
mod dep_spec;
mod exe_search;
mod package;
mod scan_fs;
mod version_spec;
use crate::scan_fs::ScanFS;

fn main() {
    let sfs = ScanFS::from_defaults().unwrap();
    sfs.report();
}
