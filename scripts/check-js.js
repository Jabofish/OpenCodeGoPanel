const { existsSync, readFileSync, readdirSync } = require('node:fs');
const { dirname, join } = require('node:path');
const { spawnSync } = require('node:child_process');

const frontendDir = join(__dirname, '..', 'src');
const sourceDir = join(frontendDir, 'js');
const moduleFiles = readdirSync(sourceDir)
  .filter(file => file.endsWith('.js'))
  .map(file => join(sourceDir, file))
  .sort();

const localImports = moduleFiles.flatMap(file => {
  const source = readFileSync(file, 'utf8');
  return [...source.matchAll(/^\s*import\s+(?:.+?\s+from\s+)?['"]([^'"]+)['"]/gm)]
    .map(match => ({ file, source: match[1] }))
    .filter(({ source }) => source.startsWith('.'));
});
const missingImports = localImports.filter(({ file, source }) =>
  !existsSync(join(dirname(file), source)),
);

if (missingImports.length > 0) {
  const details = missingImports
    .map(({ file, source }) => `${file}: ${source}`)
    .join('\n');
  console.error(`Missing local module imports:\n${details}`);
  process.exit(1);
}

const indexHtml = readFileSync(join(frontendDir, 'index.html'), 'utf8');
const scriptSources = [...indexHtml.matchAll(/<script\b[^>]*\bsrc=["']([^"']+)["']/gi)]
  .map(match => match[1])
  .filter(src => !/^(?:[a-z]+:)?\/\//i.test(src));
const missingSources = scriptSources.filter(src => !existsSync(join(frontendDir, src)));

if (missingSources.length > 0) {
  console.error(`Missing local script sources: ${missingSources.join(', ')}`);
  process.exit(1);
}

const filesToCheck = [...new Set([
  ...moduleFiles,
  ...scriptSources.map(src => join(frontendDir, src)),
])].sort();

for (const file of filesToCheck) {
  const result = spawnSync(process.execPath, ['--check', file], {
    stdio: 'inherit',
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

console.log(
  `Checked ${moduleFiles.length} frontend modules and ${scriptSources.length} entry scripts ` +
  `(${filesToCheck.length} unique JavaScript files, ${localImports.length} local imports).`,
);
