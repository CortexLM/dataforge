const fs = require('node:fs');
const assert = require('node:assert/strict');
const YAML = require('yaml');

const raw = fs.readFileSync('.coderabbit.yaml', 'utf8');
const config = YAML.parse(raw);

assert.ok(config && typeof config === 'object', 'config should parse to an object');
assert.ok(config.reviews && typeof config.reviews === 'object', 'reviews section should exist');

const reviews = config.reviews;

assert.strictEqual(reviews.review_status, true, 'review_status should be enabled');
assert.ok(!('review_status_comment' in reviews), 'review_status_comment should be removed');
assert.ok(!('auto_approve' in reviews), 'auto_approve should be removed');

assert.ok(Array.isArray(reviews.path_instructions), 'path_instructions should live under reviews');
assert.ok(!('path_instructions' in config), 'top-level path_instructions should be removed');

for (const instruction of reviews.path_instructions) {
  assert.ok(instruction && typeof instruction === 'object', 'path instruction should be an object');
  assert.strictEqual(typeof instruction.path, 'string', 'path should be a string');
  assert.ok(instruction.path.length > 0, 'path should not be empty');
  assert.strictEqual(typeof instruction.instructions, 'string', 'instructions should be a string');
  assert.ok(instruction.instructions.trim().length > 0, 'instructions should not be empty');
}

assert.ok(reviews.auto_review && typeof reviews.auto_review === 'object', 'auto_review should exist');
assert.ok(Array.isArray(reviews.auto_review.base_branches), 'base_branches should be an array');
assert.ok(reviews.auto_review.base_branches.length > 0, 'base_branches should be non-empty');
for (const branch of reviews.auto_review.base_branches) {
  assert.strictEqual(typeof branch, 'string', 'base branch entries should be strings');
  assert.ok(branch.trim().length > 0, 'base branch entries should not be empty');
}

console.log('coderabbit config validation passed');
