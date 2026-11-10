#!/usr/bin/env python3
"""Finite synthetic FIFO replay; run the whole driver via dev-session service run."""
import argparse
import json
import os
import pathlib
import random
import re
import select
import signal
import socket
import statistics
import subprocess
import tempfile
import threading
import time
import urllib.request


def scrape(url):
    started = time.monotonic()
    with urllib.request.urlopen(url, timeout=5) as response:
        text = response.read().decode()
    return text, time.monotonic() - started


def total(text, family, program=None):
    return sum(float(line.rsplit(' ', 1)[1]) for line in text.splitlines()
               if line.startswith(family + '{') and
               (program is None or 'program="' + program + '"' in line))


def write_all(fd, data, deadline):
    view = memoryview(data)
    while view:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('FIFO write deadline')
        try:
            count = os.write(fd, view)
            view = view[count:]
        except BlockingIOError:
            select.select([], [fd], [], min(remaining, .1))


def stop_child(process):
    forced = False
    if process.poll() is None:
        process.send_signal(signal.SIGCONT)
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            forced = True
    return forced


class Transport:
    def __init__(self, args, root, pipe, host, deadline):
        self.mode, self.deadline = args.transport, deadline
        self.process, self.socket, self.fd = None, None, None
        self.root = root
        if self.mode == 'direct':
            self.fd = os.open(pipe, os.O_WRONLY | os.O_NONBLOCK)
            return
        port = 0
        if self.mode in ('tcp', 'udp'):
            probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM if self.mode == 'tcp' else socket.SOCK_DGRAM)
            probe.bind(('127.0.0.1', 0))
            port = probe.getsockname()[1]
            probe.close()
        local = root / 'local.sock'
        if self.mode == 'local':
            module = 'module(load="imuxsock" SysSock.Use="off")'
            incoming = 'input(type="imuxsock" Socket=%s ruleset="remote-file")' % json.dumps(str(local))
        else:
            module = 'module(load="im%s")' % self.mode
            incoming = 'input(type="im%s" address="127.0.0.1" port="%d" ruleset="remote-file")' % (self.mode, port)
        config = ('global(localHostname=%s)\n%s\n%s\n'
                  'template(name="archive-path" type="string" string=%s)\n'
                  'ruleset(name="remote-pipe") { action(type="ompipe" Pipe=%s) }\n'
                  'ruleset(name="remote-file") { action(type="omfile" dynaFile="archive-path") call remote-pipe }\n') % (
                      json.dumps(host), module, incoming,
                      json.dumps(str(root / 'remote' / '%HOSTNAME%' / 'log')), json.dumps(str(pipe)))
        path = root / 'rsyslog.conf'
        path.write_text(config)
        args.output.with_suffix('.rsyslog.conf').write_text(config)
        self.process = subprocess.Popen([str(args.rsyslog), '-n', '-i', str(root / 'rsyslog.pid'), '-f', str(path)],
                                        stdout=open(args.output.with_suffix('.rsyslog.log'), 'wb'), stderr=subprocess.STDOUT)
        try:
            while True:
                if self.process.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError('rsyslog startup failed or timed out')
                try:
                    if self.mode == 'local':
                        self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
                        self.socket.connect(str(local))
                    else:
                        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM if self.mode == 'tcp' else socket.SOCK_DGRAM)
                        if self.mode == 'udp':
                            # UDP connect cannot prove a listener; wait for the owned daemon's socket.
                            table = pathlib.Path('/proc/net/udp').read_text()
                            if not any((':%04X' % port) in line for line in table.splitlines()):
                                raise FileNotFoundError('waiting for UDP bind')
                        self.socket.connect(('127.0.0.1', port))
                    self.socket.setblocking(False)
                    break
                except OSError:
                    self.socket.close()
                    time.sleep(.02)
        except BaseException:
            stop_child(self.process)
            raise

    def send(self, host, program, body):
        stamp = '2026-11-10T12:00:00Z'
        if self.mode == 'direct':
            write_all(self.fd, ('%s %s %s %s\n' % (stamp, host, program, body)).encode(), self.deadline)
        else:
            payload = (('<14>Nov 10 12:00:00 %s: %s' % (program, body)) if self.mode == 'local'
                       else ('<14>1 %s %s %s - - - %s' % (stamp, host, program, body))).encode()
            if self.mode == 'tcp':
                write_all(self.socket.fileno(), payload + b'\n', self.deadline)
            else:
                while True:
                    if time.monotonic() >= self.deadline:
                        raise TimeoutError('datagram send deadline')
                    try:
                        self.socket.send(payload)
                        break
                    except BlockingIOError:
                        select.select([], [self.socket], [], .1)

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
        if self.socket:
            self.socket.close()
        forced = stop_child(self.process) if self.process else False
        sequences = []
        preview = []
        lines = 0
        for path in (self.root / 'remote').rglob('log'):
            for line in path.read_text(errors='replace').splitlines():
                lines += 1
                if len(preview) < 3:
                    preview.append(line[:512])
                sequences.extend(int(n) for n in re.findall(r'\bseq=(\d+)\b', line))
        return {'rsyslog_exit_status': self.process.returncode if self.process else None,
                'rsyslog_shutdown_forced': forced,
                'archive_lines': lines if self.process else None,
                'archive_preview': preview,
                'archive_unique_sequences': len(set(sequences)) if self.process else None,
                'archive_duplicate_sequences': len(sequences) - len(set(sequences)) if self.process else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=pathlib.Path, required=True,
                        help='sanitized JSON host map used identically for both exporters')
    parser.add_argument('--transport', choices=['direct', 'tcp', 'udp', 'local'], default='direct')
    parser.add_argument('--rsyslog', type=pathlib.Path, help='exact pinned rsyslogd binary for transport fixtures')
    parser.add_argument('--burst-size', type=int, default=0, help='records per finite burst; 0 disables burst gaps')
    parser.add_argument('--burst-gap', type=float, default=0)
    parser.add_argument('--pause-seconds', type=float, default=0, help='whole-exporter SIGSTOP outage, including HTTP and expiry')
    parser.add_argument('--records', type=int, default=100000)
    parser.add_argument('--rate', type=float, default=0, help='records/s; 0 writes as fast as the FIFO accepts')
    parser.add_argument('--profile', choices=['ordinary', 'oom', 'mixed'], default='ordinary')
    parser.add_argument('--cardinality', type=int, default=1, help='distinct synthetic program labels (ordinary profile)')
    parser.add_argument('--seed', type=int, default=1)
    parser.add_argument('--port', type=int, required=True, help='unused local HTTP port')
    parser.add_argument('--pipe-size', type=int, default=1048576)
    parser.add_argument('--scrape-interval', type=float, default=1, help='seconds; 0 disables periodic scrapes')
    parser.add_argument('--timeout', type=float, default=120, help='explicit finite-run deadline')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER,
                        help='candidate command after --; {config} and {listen} are substituted')
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.records < 1 or args.rate < 0 or args.cardinality < 1 or args.timeout <= 0 or args.scrape_interval < 0:
        parser.error('supply a candidate command, positive records/cardinality/timeout and nonnegative rates')
    if (args.transport != 'direct' and args.rsyslog is None) or args.burst_size < 0 or args.burst_gap < 0 or args.pause_seconds < 0:
        parser.error('transport fixtures require --rsyslog; burst/pause values must be nonnegative')
    config = json.loads(args.config.read_text())
    hosts = list(config['hosts'])
    if not hosts:
        parser.error('host map must not be empty')
    rng = random.Random(args.seed)
    done = threading.Event()
    latencies, scrape_errors, rss, cpu = [], [], [], []
    result = {'kind': 'synthetic-' + args.transport, 'profile': args.profile,
              'seed': args.seed, 'records_requested': args.records, 'submitted_unique_sequences': 0, 'rate_requested': args.rate,
              'cardinality': args.cardinality, 'pipe_size_requested': args.pipe_size,
              'scrape_interval': args.scrape_interval, 'command': command,
              'rsyslog': str(args.rsyslog) if args.rsyslog else None,
              'burst_size': args.burst_size, 'burst_gap': args.burst_gap, 'pause_seconds': args.pause_seconds,
              'producer_accepted_unique_sequences': None, 'fifo_observed_unique_sequences': None,
              'exporter_unique_sequences': None,
              'note': 'Submission is not producer acceptance; exporter metrics expose counts, not unique sequence IDs. Local mode is socket injection, not journald end-to-end coverage. SIGSTOP also freezes HTTP/expiry.'}
    with tempfile.TemporaryDirectory(prefix='syslog-exporter-benchmark-') as tmp:
        tmp = pathlib.Path(tmp)
        pipe = tmp / 'input.pipe'
        os.mkfifo(pipe)
        config['syslog_pipe'] = str(pipe)
        config['pipe_size'] = args.pipe_size
        path = tmp / 'config.json'
        path.write_text(json.dumps(config))
        argv = [item.replace('{config}', str(path)).replace('{listen}', '127.0.0.1:' + str(args.port)) for item in command]
        result['argv'] = argv
        log_path = args.output.with_suffix('.stderr.log')
        process = subprocess.Popen(argv, stdout=open(log_path, 'wb'), stderr=subprocess.STDOUT)
        url = 'http://127.0.0.1:%d/metrics' % args.port
        deadline = time.monotonic() + args.timeout
        transport = None
        resume = None
        observer = None
        def observe():
            while not done.wait(args.scrape_interval if args.scrape_interval else .2):
                try:
                    status = pathlib.Path('/proc/%d/status' % process.pid).read_text()
                    rss.append(int(re.search(r'^VmRSS:\s+(\d+)', status, re.M)[1]) * 1024)
                    # utime/stime exclude the parent replay driver's CPU.
                    stat = pathlib.Path('/proc/%d/stat' % process.pid).read_text().rsplit(') ', 1)[1].split()
                    cpu.append((int(stat[11]) + int(stat[12])) / os.sysconf('SC_CLK_TCK'))
                    if args.scrape_interval:
                        _, elapsed = scrape(url)
                        latencies.append(elapsed)
                except (OSError, ValueError, TypeError) as error:
                    scrape_errors.append(str(error))
        try:
            while True:
                if process.poll() is not None:
                    raise RuntimeError('candidate exited before startup')
                try:
                    scrape(url)
                    transport = Transport(args, tmp, pipe, hosts[0], deadline)
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise TimeoutError('startup deadline')
                    time.sleep(.02)
            # Leading warm-up satisfies old first-line discard; measure from the subsequent scrape.
            transport.send(hosts[0], 'benchmark_discard', 'ready')
            transport.send(hosts[0], 'benchmark_warmup', 'ready')
            while True:
                initial, _ = scrape(url)
                if total(initial, 'syslog_message_count', 'benchmark_warmup:' if args.transport == 'local' else 'benchmark_warmup') >= 1:
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError('warm-up marker deadline')
                time.sleep(.01)
            initial_count = total(initial, 'syslog_message_count')
            observer = threading.Thread(target=observe)
            observer.start()
            started = time.monotonic()
            result['started_unix'] = time.time()
            for sequence in range(args.records):
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise TimeoutError('candidate exited or finite replay deadline exceeded')
                if args.pause_seconds and sequence == args.records // 2:
                    process.send_signal(signal.SIGSTOP)
                    def resume_owned():
                        if process.poll() is None:
                            process.send_signal(signal.SIGCONT)
                    resume = threading.Timer(args.pause_seconds, resume_owned)
                    resume.start()
                if args.burst_size and sequence and sequence % args.burst_size == 0:
                    time.sleep(min(args.burst_gap, max(0, deadline - time.monotonic())))
                if args.rate:
                    delay = started + sequence / args.rate - time.monotonic()
                    if delay > 0:
                        time.sleep(min(delay, max(0, deadline - time.monotonic())))
                if time.monotonic() >= deadline:
                    raise TimeoutError('candidate exited or finite replay deadline exceeded')
                host = hosts[rng.randrange(len(hosts))]
                if args.profile == 'oom':
                    program = 'kernel[]'
                    body = ('[ bench ] apache invoked oom-killer: flags' if sequence % 16 == 0
                            else '[ bench ] stack trace ' + 'x' * rng.choice([80, 500, 4000]))
                elif args.profile == 'mixed' and sequence % 4 == 0:
                    program, body = rng.choice([('osctld', "Received command 'ct.start'"), ('nodectld', 'Daemon crashed'), ('lxc', '42: Failed to spawn container \"42\"'), ('nginx', 'FastCGI sent in stderr: PHP message: PHP Fatal error: test')])
                else:
                    program = 'benchmark_%d' % (sequence % args.cardinality)
                    body = 'ordinary ' + 'x' * rng.choice([32, 128, 500])
                mark = 'seq=%d ' % sequence
                if body.startswith('[ bench ] '):
                    body = body.replace('[ bench ] ', '[ bench ] ' + mark, 1)
                elif program == 'lxc':
                    body = body.replace(': ', ': ' + mark, 1)
                else:
                    body = mark + body
                transport.send(host, program, body)
                result['submitted_unique_sequences'] += 1
            result['write_seconds'] = time.monotonic() - started
            transport.send(hosts[0], 'benchmark_marker', 'complete')
            while True:
                final, _ = scrape(url)
                if total(final, 'syslog_message_count', 'benchmark_marker:' if args.transport == 'local' else 'benchmark_marker') >= 1:
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError('final marker deadline')
                time.sleep(.01)
            result['drain_seconds'] = time.monotonic() - started
            result['accepted_records'] = total(final, 'syslog_message_count') - initial_count - 1
            result['achieved_records_per_second'] = args.records / result['drain_seconds']
            result['reader_stage_count_gap'] = args.records - result['accepted_records']
            result['effective_capacity'] = next((line.rsplit(' ', 1)[1] for line in final.splitlines()
                                                 if line.startswith('syslog_exporter_fifo_capacity_bytes ')), None)
            args.output.with_suffix('.prom').write_text(final)
            result['outcome'] = 'complete'
        except Exception as error:
            result['outcome'] = 'failed'
            result['error'] = str(error)
        finally:
            done.set()
            if observer:
                observer.join()
            if resume:
                resume.cancel()
                resume.join()
            if process.poll() is None:
                process.send_signal(signal.SIGCONT)
            # Preserve the last observable exporter state on timeout/transport failure.
            try:
                final, _ = scrape(url)
                args.output.with_suffix('.prom').write_text(final)
                result['final_exporter_count'] = total(final, 'syslog_message_count')
            except OSError as error:
                result['final_scrape_error'] = str(error)
            if transport:
                try:
                    result.update(transport.close())
                except OSError as error:
                    result['transport_cleanup_error'] = str(error)
                    if transport.process:
                        stop_child(transport.process)
            shutdown = time.monotonic()
            result['shutdown_forced'] = stop_child(process)
            result['shutdown_seconds'] = time.monotonic() - shutdown
            result['exit_status'] = process.returncode
            result['peak_rss_bytes'] = max(rss, default=None)
            result['process_cpu_seconds'] = max(cpu, default=None)
            result['scrape_count'] = len(latencies)
            result['scrape_errors'] = scrape_errors
            result['scrape_p99_seconds'] = sorted(latencies)[min(len(latencies) - 1, int(len(latencies) * .99))] if latencies else None
            result['scrape_median_seconds'] = statistics.median(latencies) if latencies else None
            args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['outcome'] == 'complete' and result['reader_stage_count_gap'] == 0 else 1


if __name__ == '__main__':
    raise SystemExit(main())
