#!/usr/bin/env python3
"""Download pinned official archives and submodules; verify every retained source hash."""
import argparse, concurrent.futures, hashlib, io, json, tarfile, urllib.request, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
RUNTIME = ROOT / 'integration/.runtime/singleton-reference'
MANIFEST = ROOT / 'integration/tools/reference/singleton-cl/source-metadata.json'
REPOS = [('v4', 'Uniswap/v4-core', 'bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6'),
         ('infinity', 'pancakeswap/infinity-core', '3c5ef1be1f71442d8bc60cc6fd11f9c677b714df')]

def get(url):
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=90) as response:
                return response.read()
        except Exception:
            if attempt == 3:
                raise
            time.sleep(attempt + 1)

def archive(local, repo, revision):
    cache = RUNTIME / 'archives' / (repo.replace('/', '--') + '--' + revision + '.tar.gz')
    cache.parent.mkdir(parents=True, exist_ok=True)
    data = cache.read_bytes() if cache.exists() else get(f'https://codeload.github.com/{repo}/tar.gz/{revision}')
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as check:
        check.getmembers()
    cache.write_bytes(data)
    files = []
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
        for member in tar:
            relative = '/'.join(member.name.split('/')[1:])
            if not member.isfile() or not relative or '..' in Path(relative).parts:
                continue
            # Keep Solidity and upstream build/remapping/license metadata, no executable scripts.
            if not relative.endswith('.sol') and relative not in ['.gitmodules', 'foundry.toml', 'remappings.txt', 'LICENSE', 'LICENSE.md']:
                continue
            content = tar.extractfile(member).read()
            name = f'{local}/{relative}'
            target = RUNTIME / 'sources' / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
            files.append(dict(local=name, repo=repo, revision=revision, path=relative,
                              sha256=hashlib.sha256(content).hexdigest()))
    return files

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--record-manifest', action='store_true', help='Explicit initial trust bootstrap; normal reproduction verifies committed hashes')
    args = parser.parse_args()
    jobs = list(REPOS)
    for local, repo, rev in REPOS:
        tree = json.loads(get(f'https://api.github.com/repos/{repo}/git/trees/{rev}?recursive=1'))
        submodules = {'forge-std': 'foundry-rs/forge-std', 'solmate': 'transmissions11/solmate',
                      'openzeppelin-contracts': 'OpenZeppelin/openzeppelin-contracts',
                      'pancake-create3-factory': 'pancakeswap/pancake-create3-factory'}
        for entry in tree['tree']:
            if entry['mode'] == '160000':
                name = entry['path'].split('/')[-1]
                jobs.append((local + '/' + entry['path'], submodules[name], entry['sha']))
    files = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
        for result in executor.map(lambda job: archive(*job), jobs):
            files.extend(result)
    manifest = dict(schema_version=1, repositories=[dict(local=l, repo=r, revision=v) for l,r,v in jobs], files=sorted(files, key=lambda f:f['local']))
    if args.record_manifest:
        MANIFEST.write_text(json.dumps(manifest, indent=2) + '\n')
    elif manifest != json.loads(MANIFEST.read_text()):
        raise SystemExit('Pinned source metadata mismatch')
    print(json.dumps(dict(primary_source_files=len(files), hashes_verified=not args.record_manifest, repositories=len(jobs))))

if __name__ == '__main__':
    main()
