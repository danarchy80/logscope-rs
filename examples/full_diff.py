import sys, zipfile, tarfile, os
sys.path.insert(0, "/home/danarchy/workspace/logscope/src")
from datetime import datetime
from pathlib import Path
from logscope.core.pipeline import run_pipeline

samples = Path("/home/danarchy/workspace/logscope/tests/samples")
out_dir = Path(os.environ["LOGSCOPE_DIFF_DIR"])

start = datetime(2026, 8, 26, 0, 0, 0)
end = datetime(2026, 8, 27, 0, 0, 0)

def run(inp, out):
    r = run_pipeline(inp, start, end, out)
    print(f"{Path(inp).name if not inp.is_dir() else 'folder'} -> sources={r.sources} total={r.total_entries} filtered={r.filtered_entries}")

run(samples / "simple.log", out_dir / "simple.out.py")
run(samples, out_dir / "folder.out.py")

zp = out_dir / "logs.zip"
with zipfile.ZipFile(zp, "w") as z:
    for f in sorted(samples.glob("*.log")):
        z.write(f, f.name)
run(zp, out_dir / "zip.out.py")

tp = out_dir / "logs.tar.gz"
with tarfile.open(tp, "w:gz") as t:
    for f in sorted(samples.glob("*.log")):
        t.add(f, f.name)
run(tp, out_dir / "tar.out.py")
