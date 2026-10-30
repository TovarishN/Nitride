#!/usr/bin/env python3
"""Run deterministic native CLI and LSP workloads for PGO and BOLT."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def invoke(binary, args, env, data=None, expected=0):
    result = subprocess.run([str(binary), *args], input=data, capture_output=True,
                            cwd=ROOT, env=env, timeout=180)
    if result.returncode != expected:
        raise RuntimeError(f'training {args}: exit {result.returncode}\n{result.stderr.decode(errors="replace")}')
    return result.stdout


def frame(message):
    body = json.dumps({'jsonrpc': '2.0', **message}).encode()
    return f'Content-Length: {len(body)}\r\n\r\n'.encode() + body


def responses(data):
    messages = []
    while data:
        headers, body = data.split(b'\r\n\r\n', 1)
        length = next(int(line.split(b':', 1)[1]) for line in headers.split(b'\r\n')
                      if line.lower().startswith(b'content-length:'))
        messages.append(json.loads(body[:length]))
        data = body[length:]
    return messages


def lsp(binary, env):
    folder = ROOT / 'languages/portable'
    source = folder / 'sample.portable'
    grammar = folder / 'Portable.ngr'
    messages = [
        {'id': 1, 'method': 'initialize', 'params': {
            'processId': None, 'rootUri': folder.as_uri(), 'capabilities': {},
            'workspaceFolders': [{'uri': folder.as_uri(), 'name': 'portable'}]}},
        {'method': 'initialized', 'params': {}},
    ]
    request_id = 2
    expected = {1}
    for file, language in [(source, 'portable'), (grammar, 'ngr')]:
        document = {'uri': file.as_uri()}
        text = file.read_text()
        messages.append({'method': 'textDocument/didOpen', 'params': {
            'textDocument': {**document, 'languageId': language, 'version': 1, 'text': text}}})
        for method, extra in [
            ('semanticTokens/full', {}), ('documentSymbol', {}), ('foldingRange', {}),
            ('hover', {'position': {'line': 0, 'character': 1}}),
            ('completion', {'position': {'line': 0, 'character': 1}}),
            ('definition', {'position': {'line': 0, 'character': 1}}),
            ('references', {'position': {'line': 0, 'character': 1}, 'context': {'includeDeclaration': True}}),
            ('formatting', {'options': {'tabSize': 2, 'insertSpaces': True}}),
        ]:
            messages.append({'id': request_id, 'method': f'textDocument/{method}',
                             'params': {'textDocument': document, **extra}})
            expected.add(request_id)
            request_id += 1
        # Broken edits exercise recovery and subsequent valid edits restore the tree.
        for revision, changed in [(2, text + '\n { ???'), (3, text + '\n')]:
            messages.append({'method': 'textDocument/didChange', 'params': {
                'textDocument': {**document, 'version': revision},
                'contentChanges': [{'text': changed}]}})
        messages.append({'method': 'textDocument/didClose', 'params': {'textDocument': document}})
    messages += [{'id': request_id, 'method': 'shutdown', 'params': None},
                 {'method': 'exit', 'params': None}]
    expected.add(request_id)
    output = responses(invoke(binary, ['lsp'], env, b''.join(map(frame, messages))))
    replies = {message['id']: message for message in output if 'id' in message}
    if not expected.issubset(replies) or any('error' in replies[i] for i in expected):
        raise RuntimeError(f'LSP training did not complete all requests: {output}')


def train(binary, env):
    invoke(binary, ['--version'], env)
    for profile in ['portable', 'power', 'composition', 'xml', 'namespaces', 'inference',
                    'calls', 'functions', 'dates', 'rewrites', 'csharp4', 'grammar']:
        invoke(binary, ['prepare', f'languages/{profile}/project.json'], env)
    for args in [
        ['languages/portable/sample.portable'],
        ['--power', 'languages/power/sample.powercalc'],
        ['--parse-xml', 'languages/xml/sample.xml'],
        ['--parse-composition', 'languages/composition/sample.composed'],
        ['--namespaces', 'languages/namespaces/sample.namespaces'],
        ['--inference', 'languages/inference/sample.inferred'],
        ['--calls', 'languages/calls/sample.calls'],
        ['--functions', 'languages/functions/sample.functions'],
        ['--dates', 'languages/dates/sample.dates', '--today', '2026-10-24'],
        ['--rewrites', 'languages/rewrites/sample.rewrites', '--today', '2026-10-24'],
        ['--parse-csharp4', 'languages/csharp4/corpus/Mono2008/gtest-linq-01.cs'],
    ]:
        invoke(binary, args, env)
    # More weight on warm parser/compiler execution, without process-start bias.
    invoke(binary, ['--bench', 'languages/portable/sample.portable', '100'], env)
    for profile, source in [('portable', 'let x = ; show x;'), ('csharp4', 'class C { void M( {')]:
        request = json.dumps({'profile': profile, 'source': source}).encode() + b'\n'
        invoke(binary, ['--recover-batch'], env, request)
    lsp(binary, env)
