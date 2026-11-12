"""Finite tests of the actual updater shell, with real disposable Git remotes."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent


def block(marker):
    lines = (ROOT / '.github/workflows/update-dependencies.yml').read_text().split(marker, 1)[1].splitlines()
    start = next(i for i, line in enumerate(lines) if line == '        run: |') + 1
    code = []
    for line in lines[start:]:
        if line and not line.startswith('          '):
            break
        code.append(line[10:])
    return '\n'.join(code) + '\n'


UPDATE = block('        id: update')
PUSH = block('      - name: Push tested lockfile')


class UpdateWorkflow(unittest.TestCase):
    def exercise(self, mode='lock', dry=False, failure=False, race=False, stale=False, fallback=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_') and k not in ('GITHUB_TOKEN', 'GH_TOKEN')}
            env.update(GIT_CONFIG_GLOBAL=str(root / 'gitconfig'), GIT_CONFIG_NOSYSTEM='1')
            def git(*args, cwd=root):
                return subprocess.check_output(['git', *map(str, args)], cwd=cwd, env=env, stderr=subprocess.STDOUT).decode().strip()
            def commit(cwd):
                message = root / 'fixture-message'
                message.write_text('Fixture\n')
                git('add', '-A', cwd=cwd)
                git('commit', '-q', '-F', message, cwd=cwd)
                return git('rev-parse', 'HEAD', cwd=cwd)
            git('config', '--global', 'user.name', 'fixture')
            git('config', '--global', 'user.email', 'fixture@example.invalid')
            origin, repo, advisory = root / 'origin.git', root / 'repo', root / 'advisory'
            git('init', '-q', '--bare', '--initial-branch=master', origin)
            git('clone', '-q', origin, repo)
            (repo / 'Cargo.lock').write_text('version = 4\n')
            (repo / 'Cargo.toml').write_text('[package]\nname = "fixture"\n')
            base = commit(repo)
            git('push', '-q', 'origin', 'master', cwd=repo)
            git('init', '-q', advisory)
            (advisory / 'database').write_text('fixture')
            commit(advisory)
            git('config', '--global', 'url.' + advisory.as_uri() + '.insteadOf', 'https://github.com/RustSec/advisory-db.git')
            tools = root / 'tools'
            tools.mkdir()
            stub = tools / 'nix'
            stub.write_text('#!' + sys.executable + '\n' + '''import json,os,pathlib,sys
args=sys.argv[1:]
log=pathlib.Path(os.environ['FIXTURE_CALLS'])
with log.open('a') as out:out.write(json.dumps(args)+'\\n')
if args[0]=='develop' and 'update' in args:
 p=pathlib.Path('Cargo.lock');mode=os.environ['FIXTURE_MODE']
 if mode!='none':p.write_text(p.read_text()+'# compatible update\\n')
 if mode=='extra':pathlib.Path('Cargo.toml').write_text('unexpected')
 if mode=='mode':p.chmod(0o755)
elif args[0]=='develop' and 'audit' in args:
 assert '--no-fetch' in args
elif args[:3]==['config','show','--json']:print(json.dumps({'sandbox':{'value':True},'sandbox-fallback':{'value':os.environ['FIXTURE_FALLBACK']=='true'}}))
elif args[0]=='build':
 assert '--option' in args and 'true' in args and '--print-build-logs' in args
 assert args[args.index('sandbox-fallback')+1]=='false'
 assert not os.environ.get('GITHUB_TOKEN')
 if os.environ['FIXTURE_FAIL']=='true':sys.exit(1)
else:sys.exit(2)
''')
            stub.chmod(0o755)
            runner = root / 'runner'
            runner.mkdir()
            output = root / 'outputs'
            env.update(PATH=str(tools) + os.pathsep + env['PATH'], FIXTURE_CALLS=str(root / 'calls'),
                       FIXTURE_MODE=mode, FIXTURE_FAIL=str(failure).lower(), FIXTURE_FALLBACK=str(fallback).lower(), RUNNER_TEMP=str(runner),
                       GITHUB_OUTPUT=str(output), GITHUB_SHA=base, GITHUB_REF='refs/heads/master',
                       GITHUB_REPOSITORY='vpsfreecz/syslog-exporter', DRY_RUN=str(dry).lower())
            result = subprocess.run(['bash', '-c', UPDATE], cwd=repo, env=env, capture_output=True, text=True)
            fields = dict(line.split('=', 1) for line in output.read_text().splitlines())
            if failure or fallback or mode in ('extra', 'mode'):
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(git('rev-parse', 'HEAD', cwd=repo), base)
                self.assertNotIn('commit', fields)
            elif dry or mode == 'none':
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(git('rev-parse', 'HEAD', cwd=repo), base)
                self.assertNotIn('commit', fields)
            else:
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                head = fields['commit']
                self.assertEqual(git('rev-parse', 'HEAD^', cwd=repo), base)
                self.assertEqual(git('diff', '--name-only', base, head, cwd=repo), 'Cargo.lock')
                identity = git('show', '-s', '--format=%an <%ae>%n%cn <%ce>', cwd=repo).splitlines()
                self.assertEqual(identity, ['github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>'] * 2)
                env.update(BASE_SHA=base, COMMIT_SHA=head, GITHUB_TOKEN='fixture-token')
                if stale or race:
                    competitor = root / 'competitor'
                    git('clone', '-q', origin, competitor)
                    (competitor / 'other').write_text('competing default update')
                    other = commit(competitor)
                    if stale:
                        git('push', '-q', 'origin', 'master', cwd=competitor)
                    else:
                        hook = runner / 'ci-hooks/pre-push'
                        hook.write_text('#!' + sys.executable + '\nimport os,subprocess\ne={k:v for k,v in os.environ.items() if not k.startswith("GIT_")}\nsubprocess.run(["git","-C",' + repr(str(competitor)) + ',"push","-q","origin","master"],env=e,check=True)\n')
                        hook.chmod(0o755)
                pushed = subprocess.run(['bash', '-c', PUSH], cwd=repo, env=env, capture_output=True, text=True)
                self.assertEqual(git('rev-parse', 'master', cwd=origin), other if stale or race else head)
                self.assertEqual(pushed.returncode == 0, not (stale or race), pushed.stdout + pushed.stderr)
                if race:
                    self.assertIn('push_exit=1', pushed.stdout)
            if fallback:
                self.assertFalse(any(json.loads(line)[0] == 'build' for line in (root / 'calls').read_text().splitlines()))
            if failure or fallback or dry or mode in ('none', 'extra', 'mode'):
                self.assertEqual(git('rev-parse', 'master', cwd=origin), base)

    def test_no_change(self): self.exercise(mode='none')
    def test_failed_check(self): self.exercise(failure=True)
    def test_sandbox_fallback_refused(self): self.exercise(fallback=True)
    def test_only_tested_lock_and_bot_commit(self): self.exercise()
    def test_extra_tracked_change(self): self.exercise(mode='extra')
    def test_mode_change(self): self.exercise(mode='mode')
    def test_dry_run(self): self.exercise(dry=True)
    def test_stale_default(self): self.exercise(stale=True)
    def test_nonforce_push_race(self): self.exercise(race=True)


if __name__ == '__main__':
    unittest.main()
