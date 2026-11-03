#!/usr/bin/env python3
"""Exercise the real demo handlers through the CLI's trusted input script."""
from pathlib import Path
import json
import subprocess

EXAMPLE = Path(__file__).resolve().parent
ROOT = EXAMPLE.parent.parent
command = [str(ROOT / 'target/debug/varta'), 'dev', '--config', str(EXAMPLE / 'varta.config.json'),
           '--view-script', str(EXAMPLE / 'interactions.json')]
result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=True)
snapshot, end = json.JSONDecoder().raw_decode(result.stdout)
assert result.stdout[end:].strip() == 'varta dev: completed', result.stdout[end:]
nodes = []
pending = [snapshot['tree']]
while pending:
    node = pending.pop()
    nodes.append(node)
    pending.extend(node.get('children', []))
texts = [node['text'] for node in nodes if 'text' in node]
identified = {node['attributes']['id']: node for node in nodes if 'id' in node.get('attributes', {})}
assert '5' in texts
assert 'Вітаю, Павло!' in texts
assert identified['name']['value'] == 'Павло'
assert identified['editor']['value'].endswith(' Готово!')
assert f"Довжина нотатки: {len(identified['editor']['value'])} символів" in texts
assert identified['highlight']['checked'] is True
assert identified['step-five']['checked'] is True
assert identified['step-one']['checked'] is False
assert identified['topic']['value'] == 'core'
assert 'Обрано: ядро' in texts
assert 'JSON: Varta, елементів: 3\nГенератор: 2 + 3 + 5 = 10\nPromise / await: 7\nУсі обчислення завершено.' in texts
assert any(node.get('class') == 'card selected' for node in nodes)
print('Showcase: 12 trusted inputs, counter/radio/checkbox/edit/select/JSON/generator/async assertions passed')
