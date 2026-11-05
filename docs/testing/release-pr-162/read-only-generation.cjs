const fs = require('fs');
const {GitHub, Manifest, VERSION} = require('release-please');
const token = fs.readFileSync(0, 'utf8').trim();
const requests = [];
const readOnlyFetch = async (url, opts = {}) => {
  const method = (opts.method || 'GET').toUpperCase();
  const parsed = new URL(typeof url === 'string' ? url : url.url);
  let operation = '';
  if (!['GET', 'HEAD'].includes(method)) {
    if (method !== 'POST' || parsed.pathname !== '/graphql') throw new Error(`WRITE BLOCKED ${method} ${parsed.pathname}`);
    const payload = JSON.parse(opts.body || '{}');
    if (!payload.query || /\bmutation\b/.test(payload.query)) throw new Error('GraphQL mutation blocked');
    operation = payload.query.slice(0, 90);
  }
  requests.push({method, path: parsed.pathname, operation});
  return fetch(url, opts);
};
(async () => {
  const github = await GitHub.create({owner: 'rabii-chaarani', repo: 'codebaseGraph', token, defaultBranch: 'main', fetch: readOnlyFetch});
  const scenarios = [
    {name: 'bootstrap-failed-4-merge', overrides: {bootstrapSha: '7038e66ff8271ffddc511f879cf8d28353a09b2d'}},
    {name: 'bootstrap-last-published', overrides: {bootstrapSha: 'ca115690b226384164cf364458a0e3255b8908d3'}},
  ];
  for (const scenario of scenarios) {
    const manifest = await Manifest.fromManifest(github, 'main', 'release-please-config.json', '.release-please-manifest.json', scenario.overrides);
    const prs = await manifest.buildPullRequests();
    const results = [];
    for (const pr of prs) {
      const changes = await github.buildChangeSet(pr.updates, 'main');
      results.push({title: pr.title.toString(), body: pr.body.toString(), version: pr.version.toString(), updates: [...changes].map(([path, change]) => ({path, content: change.content}))});
    }
    fs.writeFileSync(`./${scenario.name}.json`, JSON.stringify({releasePleaseVersion: VERSION, bootstrapSha: scenario.overrides.bootstrapSha, results, requests}, null, 2));
    console.log('SCENARIO', scenario.name, results.map(r => ({title: r.title, version: r.version, body: r.body, paths: r.updates.map(u => u.path)})));
  }
})().catch(e => {console.error(e.stack || e.message); process.exitCode = 1;});
