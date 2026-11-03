#!/usr/bin/env python3
"""Package this example with the experimental native preview, for local macOS use."""
import argparse
import json
from pathlib import Path
import os
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile

EXAMPLE = Path(__file__).resolve().parent
ROOT = EXAMPLE.parent.parent
FONT = Path('/System/Library/Fonts/Supplemental/Arial.ttf')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=['debug', 'release'], default='debug')
    parser.add_argument('--variant', default='', help='optional local bundle variant, e.g. tsx')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--renderer', choices=['cpu', 'gpu', 'auto'], default='cpu')
    parser.add_argument('--timings', action='store_true', help='emit host timing diagnostics to stderr')
    parser.add_argument('--skip-host-build', action='store_true', help='use already built hosts for the selected profile')
    args = parser.parse_args()
    if args.variant and not re.fullmatch(r'[a-z][a-z0-9-]{0,31}', args.variant):
        parser.error('variant must be a short lowercase name')
    if sys.platform != 'darwin':
        parser.error('this bundle builder targets macOS; use varta-preview directly on other platforms')
    suffix = f' {args.variant.upper()}' if args.variant else ''
    output = (args.output or ROOT / 'dist' / f'Varta Showcase{suffix} {args.profile.title()}.app').resolve()
    if output.exists():
        parser.error(f'{output} already exists; choose another --output')
    if not FONT.is_file():
        parser.error(f'the explicit demo font is missing: {FONT}')
    if not args.skip_host_build:
        subprocess.run(['cargo', 'build', '-p', 'varta', '--bin', 'varta', '--locked'], cwd=ROOT, check=True)
        command = ['cargo', 'build', '-p', 'varta-desktop', '--bin', 'varta-preview', '--locked']
        if args.renderer != 'cpu':
            command.extend(['--features', 'gpu'])
        if args.profile == 'release':
            command.append('--release')
        subprocess.run(command, cwd=ROOT, check=True)
    cli = ROOT / 'target/debug/varta'
    preview = ROOT / 'target' / args.profile / 'varta-preview'
    if not cli.is_file() or not preview.is_file():
        parser.error('build varta and varta-preview first')
    if args.renderer != 'cpu':
        info = json.loads(subprocess.check_output([str(preview), '--renderer-info'], text=True))
        if info.get('schemaVersion') != 1 or info.get('gpuCompiled') is not True:
            parser.error('the selected host has no GPU backend; build with --features gpu')
    subprocess.run([str(cli), 'build', '--config', str(EXAMPLE / 'varta.config.json')], cwd=ROOT, check=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.varta-showcase-', dir=output.parent) as temp:
        bundle = Path(temp) / output.name
        contents = bundle / 'Contents'
        macos = contents / 'MacOS'
        resources = contents / 'Resources'
        macos.mkdir(parents=True)
        resources.mkdir()
        host = macos / 'varta-preview'
        shutil.copy2(preview, host)
        if args.profile == 'release':
            subprocess.run(['/usr/bin/strip', '-S', '-x', str(host)], check=True)
        subprocess.run(['/usr/bin/codesign', '--force', '--sign', '-', '--timestamp=none', str(host)], check=True)
        for source in ['showcase.wasm', 'showcase.manifest.json']:
            shutil.copy2(EXAMPLE / 'dist' / source, resources / source)
        # The font is copied only into the ignored local bundle, never the source tree.
        shutil.copyfile(FONT, resources / 'Arial.ttf')
        launcher = macos / 'launch'
        launcher.write_text(f'''#!/bin/sh
set -eu
export VARTA_PREVIEW_RENDERER={args.renderer}
export VARTA_PREVIEW_TIMING={1 if args.timings else 0}
varta_contents=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec "$varta_contents/MacOS/varta-preview" "$varta_contents/Resources/showcase.wasm" "$varta_contents/Resources/Arial.ttf" Arial
''')
        launcher.chmod(0o755)
        with (contents / 'Info.plist').open('wb') as stream:
            plistlib.dump({
                'CFBundleExecutable': 'launch',
                'CFBundleIdentifier': f'local.varta.showcase.{args.variant + "." if args.variant else ""}{args.profile}',
                'CFBundleName': output.stem,
                'CFBundleDisplayName': output.stem,
                'CFBundlePackageType': 'APPL',
                'CFBundleVersion': '1',
                'CFBundleShortVersionString': '0.1',
                'NSHighResolutionCapable': True,
                'NSPrincipalClass': 'NSApplication',
            }, stream)
        (resources / 'README.txt').write_text(
            'Varta interactive native showcase. Local experimental preview.\n'
            f'Renderer preference: {args.renderer}; CPU fallback available.\n'
            f'Host build profile: {args.profile}.\n'
            'Wasm application; no network, filesystem or clipboard grants.\n'
            'Arial was copied from this Mac for the local demo, not vendored in Varta.\n'
            'Close the window to exit. Tab/Shift+Tab navigate controls.\n')
        os.rename(bundle, output)
    print(output)


if __name__ == '__main__':
    main()
