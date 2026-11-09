import * as fs from 'node:fs';
import * as path from 'node:path';
export interface TestCase {
    name: string;
    span: [
        number,
        number
    ];
    name_span: [
        number,
        number
    ];
    phase: string;
    source: {
        file?: string;
        text?: string;
        raw_span?: [
            number,
            number
        ] | null;
    };
}
export interface SuiteInfo {
    path: string;
    project: string;
    dependencies: string[];
    tests: TestCase[];
    project_error?: string | null;
}
export interface Result {
    suite: string;
    name: string;
    status: 'passed' | 'failed' | 'errored';
    message?: string;
    actual?: unknown;
    expected?: unknown;
}
export interface Report {
    version: number;
    exit_code: number;
    results: Result[];
    errors: {
        message: string;
    }[];
    counts: {
        passed: number;
        failed: number;
        errored: number;
    };
}
function object(value: unknown): Record<string, unknown> {
    if (!value || typeof value !== 'object' || Array.isArray(value))
        throw new Error('Expected protocol object');
    return value as Record<string, unknown>;
}
function span(value: unknown): value is [
    number,
    number
] { return Array.isArray(value) && value.length === 2 && value.every(v => Number.isSafeInteger(v) && v >= 0) && value[0] <= value[1]; }
function result(value: unknown): Result {
    const r = object(value);
    if (typeof r.suite !== 'string' || typeof r.name !== 'string' || !['passed', 'failed', 'errored'].includes(String(r.status)))
        throw new Error('Invalid test result');
    return r as unknown as Result;
}
export function listResponse(value: unknown): {
    suites: SuiteInfo[];
    errors: {
        path?: string;
        span?: [
            number,
            number
        ];
        message: string;
    }[];
} {
    const r = object(value);
    if (r.version !== 1 || !Array.isArray(r.suites) || !Array.isArray(r.errors))
        throw new Error('Invalid test discovery response');
    const suites = r.suites.map(value => {
        const s = object(value);
        if (typeof s.path !== 'string' || typeof s.project !== 'string' || !Array.isArray(s.dependencies) || !s.dependencies.every(p => typeof p === 'string') || !Array.isArray(s.tests) || s.tests.length > 1000)
            throw new Error('Invalid test suite');
        const names = new Set<string>();
        for (const item of s.tests) {
            const c = object(item);
            if (typeof c.name !== 'string' || !c.name || names.has(c.name) || !span(c.span) || !span(c.name_span) || !['parse', 'compile', 'run'].includes(String(c.phase)))
                throw new Error('Invalid test case');
            names.add(c.name);
            const source = object(c.source);
            if ((typeof source.file === 'string') === (typeof source.text === 'string'))
                throw new Error('Invalid test source');
            if (source.raw_span != null && !span(source.raw_span))
                throw new Error('Invalid raw source span');
        }
        return s as unknown as SuiteInfo;
    });
    const errors = r.errors.map(e => {
        const v = object(e);
        if (typeof v.message !== 'string' || (v.span !== undefined && !span(v.span)))
            throw new Error('Invalid discovery error');
        return v as {
            path?: string;
            span?: [
                number,
                number
            ];
            message: string;
        };
    });
    return { suites, errors };
}
export class EventStream {
    private started = false;
    private ended = false;
    private results = new Map<string, Result>();
    private report?: Report;
    constructor(private suite: string, private names: string[]) {
        if (new Set(names).size !== names.length)
            throw new Error('Duplicate requested test');
    }
    accept(value: unknown): Result | undefined {
        const event = object(value);
        if (event.version !== 1 || this.ended)
            throw new Error('Invalid or late test event');
        if (event.event === 'runStarted') {
            if (this.started)
                throw new Error('Duplicate run start');
            this.started = true;
            return;
        }
        if (!this.started)
            throw new Error('Test event before run start');
        if (event.event === 'caseFinished') {
            const r = result(event.result);
            if (r.suite !== this.suite || !this.names.includes(r.name) || this.results.has(r.name))
                throw new Error('Unexpected/duplicate test result');
            this.results.set(r.name, r);
            return r;
        }
        if (event.event !== 'runFinished')
            throw new Error('Unknown test event');
        const r = object(event.report);
        if (r.version !== 1 || !Array.isArray(r.results) || !Array.isArray(r.errors) || !r.errors.every(e => typeof object(e).message === 'string'))
            throw new Error('Invalid test report');
        const results = r.results.map(result);
        if (results.length !== this.results.size || results.some(x => JSON.stringify(x) !== JSON.stringify(this.results.get(x.name))))
            throw new Error('Report differs from streamed results');
        const counts = { passed: results.filter(r => r.status === 'passed').length, failed: results.filter(r => r.status === 'failed').length, errored: results.filter(r => r.status === 'errored').length + r.errors.length };
        const code = counts.errored ? 2 : counts.failed ? 1 : 0;
        if (r.exit_code !== code)
            throw new Error('Inconsistent report exit code');
        const actual = object(r.counts);
        if (Object.entries(counts).some(([k, v]) => actual[k] !== v))
            throw new Error('Inconsistent report counts');
        if (results.length !== this.names.length && !r.errors.length)
            throw new Error('Missing test results');
        this.ended = true;
        this.report = r as unknown as Report;
    }
    finish(code: number | null): Report {
        if (!this.report || code !== this.report.exit_code)
            throw new Error('Incomplete test stream or abnormal worker exit');
        return this.report;
    }
}
export function physicalPath(file: string): string {
    if (!path.isAbsolute(file))
        throw new Error('Expected absolute physical path');
    let canonical: string;
    try {
        canonical = fs.realpathSync.native(file);
    }
    catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT')
            throw error;
        canonical = path.join(fs.realpathSync.native(path.dirname(file)), path.basename(file));
    }
    return canonical.replace(/^\\\\\?\\/, '');
}
export function withinRoots(file: string, roots: string[]): boolean {
    try {
        const canonical = physicalPath(file);
        return roots.some(root => {
            let a = physicalPath(root), b = canonical;
            if (process.platform === 'win32') {
                a = a.toLowerCase();
                b = b.toLowerCase();
            }
            const relative = path.relative(a, b);
            return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
        });
    }
    catch {
        return false;
    }
}
export function bytePosition(text: string, offset: number): {
    line: number;
    character: number;
} {
    const bytes = Buffer.from(text);
    if (!Number.isSafeInteger(offset) || offset < 0 || offset > bytes.length || offset < bytes.length && (bytes[offset] & 0xc0) === 0x80)
        throw new Error('Invalid UTF-8 source position');
    const prefix = bytes.subarray(0, offset).toString('utf8');
    const lines = prefix.split('\n');
    return { line: lines.length - 1, character: lines[lines.length - 1].length };
}
export function selectedNames(all: string[], include: string[] | undefined, exclude: string[]): string[] { return all.filter(n => (!include || include.includes(n)) && !exclude.includes(n)); }
export function readInput(file: string): string {
    const handle = fs.openSync(file, 'r');
    try {
        const chunks: Buffer[] = [];
        let size = 0;
        for (;;) {
            const chunk = Buffer.allocUnsafe(65536);
            const count = fs.readSync(handle, chunk, 0, chunk.length, null);
            if (!count)
                break;
            size += count;
            if (size > 16 * 1024 * 1024)
                throw new Error('Test input exceeds16 MiB');
            chunks.push(chunk.subarray(0, count));
        }
        return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(Buffer.concat(chunks));
    }
    finally {
        fs.closeSync(handle);
    }
}
export function diagnosticLocation(text: string, raw: [
    number,
    number
] | null | undefined, diagnostic: [
    number,
    number
]): {
    start: {
        line: number;
        character: number;
    };
    end: {
        line: number;
        character: number;
    };
} | undefined {
    if (!raw)
        return;
    if (!span(raw) || !span(diagnostic) || diagnostic[1] > raw[1] - raw[0])
        throw new Error('Invalid raw diagnostic range');
    return { start: bytePosition(text, raw[0] + diagnostic[0]), end: bytePosition(text, raw[0] + diagnostic[1]) };
}
