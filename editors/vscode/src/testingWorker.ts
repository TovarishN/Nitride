import { spawn } from 'node:child_process';
import { EventStream, Report, Result } from './testingProtocol';
const LIMIT = 32 * 1024 * 1024;
/** One bounded, cancellable native worker. Arguments never pass through a shell. */
export function nativeWorker(command: string, args: string[], snapshot: unknown, signal: AbortSignal, onLine?: (value: unknown) => void): Promise<{
    text: string;
    code: number | null;
}> {
    return new Promise((resolve, reject) => {
        if (signal.aborted) {
            reject(new Error('Test run cancelled'));
            return;
        }
        const child = spawn(command, args, { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, shell: false });
        let output = '', stderr = '', line = '', size = 0, failure: Error | undefined;
        let timer: ReturnType<typeof setTimeout> | undefined;
        const stop = () => { child.kill(); timer = setTimeout(() => child.kill('SIGKILL'), 1000); timer.unref(); };
        const abort = () => { failure = new Error('Test run cancelled'); stop(); };
        signal.addEventListener('abort', abort, { once: true });
        child.stdout.setEncoding('utf8');
        child.stderr.setEncoding('utf8');
        child.stdout.on('data', (chunk: string) => {
            size += Buffer.byteLength(chunk);
            if (size > LIMIT) {
                failure = new Error('Test output exceeds32 MiB');
                stop();
                return;
            }
            if (onLine) {
                line += chunk;
                let end;
                while ((end = line.indexOf('\n')) >= 0) {
                    const part = line.slice(0, end);
                    line = line.slice(end + 1);
                    try {
                        if (part.trim())
                            onLine(JSON.parse(part));
                    }
                    catch (error) {
                        failure = error instanceof Error ? error : new Error(String(error));
                        stop();
                    }
                }
            }
            else {
                output += chunk;
            }
        });
        child.stderr.on('data', (chunk: string) => {
            if (stderr.length < 65536)
                stderr += chunk;
        });
        child.on('error', error => { failure = error; });
        child.stdin.on('error', error => {
            if (!failure)
                failure = error;
        });
        child.on('close', code => {
            signal.removeEventListener('abort', abort);
            if (timer)
                clearTimeout(timer);
            if (failure)
                reject(failure);
            else if (onLine && line.trim())
                reject(new Error('Truncated test event stream'));
            else if (!onLine && code !== 0 && code !== 2)
                reject(new Error(stderr || `Native test worker exited ${code}`));
            else
                resolve({ text: output, code });
        });
        child.stdin.end(JSON.stringify(snapshot));
    });
}
export async function executeTests(command: string, suite: string, names: string[], snapshot: unknown, signal: AbortSignal, onResult: (result: Result) => void): Promise<Report> {
    const stream = new EventStream(suite, names);
    const args = ['test', suite, '--events', '--snapshot-stdin', ...names.flatMap(name => ['--case', name])];
    const { code } = await nativeWorker(command, args, snapshot, signal, event => {
        const r = stream.accept(event);
        if (r)
            onResult(r);
    });
    return stream.finish(code);
}
/** Coalesce edits per suite while preserving independent refreshes. */
export class SuiteRefresh {
    private timers = new Map<string, ReturnType<typeof setTimeout>>();
    constructor(private delay = 150) { }
    schedule(key: string, work: () => void): void { const previous = this.timers.get(key); if (previous)
        clearTimeout(previous); this.timers.set(key, setTimeout(() => { this.timers.delete(key); work(); }, this.delay)); }
    dispose(): void { for (const timer of this.timers.values())
        clearTimeout(timer); this.timers.clear(); }
}
