/// Model only the Nix sandbox's numeric store-root owner, retaining real modes
/// and alias admission. Production code never consults this test fixture.
pub fn script(body: &str) -> String {
    let uid = option_env!("HARBOR_DB_TEST_ROOT_UID").unwrap_or("0");
    format!(
        r#"import os
from pathlib import Path
from unittest.mock import patch
store_uid = int({uid:?})
if store_uid:
    assert store_uid != os.geteuid(), 'builder ownership is not store-root ownership'
    original_store_lstat = Path.lstat
    def store_lstat(path, *args, **kwargs):
        info = original_store_lstat(path, *args, **kwargs)
        if not path.is_relative_to('/nix/store') or info.st_uid != store_uid:
            return info
        fields = list(info)
        fields[4] = 0
        return os.stat_result(fields)
    patch.object(Path, 'lstat', store_lstat).start()
{body}"#
    )
}
