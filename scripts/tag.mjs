// 판을 상승시키고 그 커밋에 표식한다.
//
//   npm run tag -- patch     (0.0.24 → 0.0.25)
//   npm run tag -- minor | major
//   npm run tag -- 1.2.3     (명시)
//
// **반출의 입구가 하나인 사유다.** 판은 `crates/tauri/tauri.conf.json` 한 곳에
// 소재하고 태그는 그것과 동일한 글자여야 하는데, 양측을 사람이 수동으로
// 기재하면 한쪽만 기재된다 — 그러면 반출이 중도에 정지하고 태그 하나가
// 소모된다. 그래서 상승·커밋·표식을 하나의 명령이 수행한다.
//
// **송출하지는 않는다.** 송출과 반출은 사람이 기동한다(`docs/desktop.md`).

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const CONF = 'crates/tauri/tauri.conf.json';
const VERSION = /^(\d+)\.(\d+)\.(\d+)$/;

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();

const stop = (why) => {
  console.error(why);
  process.exit(1);
};

const asked = process.argv[2];
if (!asked) {
  stop(`무엇으로 상승시킬지 기재하십시오: npm run tag -- patch | minor | major | <판>`);
}

// 판이 소재하는 곳은 한 곳이다. 글자로 치환해 그 파일의 나머지 서식을 존치한다.
const source = readFileSync(CONF, 'utf8');
const at = source.match(/"version":\s*"([^"]+)"/);
if (!at) {
  stop(`${CONF} 에서 판을 탐색하지 못했습니다`);
}
const now = at[1];
const parts = now.match(VERSION);
if (!parts) {
  stop(`${CONF} 의 판이 x.y.z 가 아닙니다: ${now}`);
}

const [major, minor, patch] = parts.slice(1).map(Number);
const next = {
  major: `${major + 1}.0.0`,
  minor: `${major}.${minor + 1}.0`,
  patch: `${major}.${minor}.${patch + 1}`,
}[asked] ?? asked;

if (!VERSION.test(next)) {
  stop(`판이 x.y.z 가 아닙니다: ${next}`);
}
if (next === now) {
  stop(`판이 이미 ${next} 입니다`);
}

// 상승하지 않는 판은 이미 반출된 것을 가리킨다.
const rank = (v) => v.split('.').map(Number);
const [a, b, c] = rank(next);
const [x, y, z] = rank(now);
if (a * 1e6 + b * 1e3 + c <= x * 1e6 + y * 1e3 + z) {
  stop(`${next} 는 현재의 ${now} 보다 상위가 아닙니다`);
}

// **무관한 변경을 태그에 수용하지 않는다.** 작업 중인 것이 잔존한 채로 표식하면
// 반출되는 것에 그것이 포함된다.
if (git('status', '--porcelain') !== '') {
  stop('작업 디렉터리가 청결하지 않습니다 — 커밋하거나 보류한 뒤 재시도하십시오');
}

// 이미 소재하는 태그를 전치하면 반출된 것의 내용이 변동한다.
const tags = git('tag', '--list').split('\n').map((t) => t.trim());
if (tags.includes(next)) {
  stop(`태그 ${next} 가 이미 소재합니다`);
}

writeFileSync(CONF, source.replace(at[0], `"version": "${next}"`));

git('add', '--', CONF);
git('commit', '-m', `tauri.conf.json 의 판을 ${next} 로 상승시킨다`);
git('tag', next);

const head = git('rev-parse', '--short', 'HEAD');
console.log(`판 ${now} → ${next}, ${head} 에 태그 ${next}`);
console.log(`송출: git push origin HEAD ${next}`);
console.log(`반출: gh workflow run publish --ref ${next}`);
