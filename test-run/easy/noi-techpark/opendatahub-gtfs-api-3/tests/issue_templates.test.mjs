import assert from 'assert/strict'
import fs from 'fs/promises'
import path from 'path'
import yaml from 'js-yaml'

const repoRoot = process.cwd()

async function loadTemplate (relativePath) {
  const fullPath = path.join(repoRoot, relativePath)
  const content = await fs.readFile(fullPath, 'utf8')
  return yaml.load(content)
}

function findBodyEntry (body, id) {
  return body.find((entry) => entry.id === id)
}

const bugTemplate = await loadTemplate('.github/ISSUE_TEMPLATE/1-bug-report.yml')
assert.equal(typeof bugTemplate.name, 'string')
assert.equal(typeof bugTemplate.description, 'string')
assert.ok(Array.isArray(bugTemplate.labels), 'bug template labels should be an array')
assert.ok(bugTemplate.labels.includes('bug'), 'bug template should include bug label')
assert.ok(Array.isArray(bugTemplate.body), 'bug template body should be an array')

const bugRequiredIds = ['problem', 'where', 'how', 'steps', 'expected', 'actual', 'reproduce']
for (const id of bugRequiredIds) {
  const entry = findBodyEntry(bugTemplate.body, id)
  assert.ok(entry, `bug template missing body entry for ${id}`)
  assert.equal(entry.validations?.required, true, `bug template ${id} should be required`)
}

const markdownEntry = bugTemplate.body.find((entry) => entry.type === 'markdown')
assert.ok(markdownEntry && markdownEntry.attributes?.value, 'bug template should include markdown instructions')

const featureTemplate = await loadTemplate('.github/ISSUE_TEMPLATE/2-feature-request.yml')
assert.equal(typeof featureTemplate.name, 'string')
assert.equal(typeof featureTemplate.description, 'string')
assert.ok(Array.isArray(featureTemplate.body), 'feature template body should be an array')

const featureRequiredIds = ['problem', 'solution']
for (const id of featureRequiredIds) {
  const entry = findBodyEntry(featureTemplate.body, id)
  assert.ok(entry, `feature template missing body entry for ${id}`)
  assert.equal(entry.validations?.required, true, `feature template ${id} should be required`)
}

const alternativesEntry = findBodyEntry(featureTemplate.body, 'alternatives')
assert.ok(alternativesEntry, 'feature template missing alternatives entry')
assert.equal(alternativesEntry.validations?.required, false, 'alternatives should not be required')

const featureMarkdown = featureTemplate.body.find((entry) => entry.type === 'markdown')
assert.ok(featureMarkdown && featureMarkdown.attributes?.value, 'feature template should include markdown instructions')
