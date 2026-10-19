const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

test('mobile responsiveness layout audit (< 640px)', (t) => {
  const indexPage = fs.readFileSync(path.join(__dirname, '../pages/index.tsx'), 'utf8');
  assert.ok(
    indexPage.includes('flex flex-col sm:flex-row'),
    'Header on index.tsx adapts to vertical on mobile (< 640px)'
  );

  const skeleton = fs.readFileSync(path.join(__dirname, '../components/ResultViewerSkeleton.tsx'), 'utf8');
  assert.ok(
    skeleton.includes('grid-cols-4 sm:grid-cols-6') || skeleton.includes('grid-cols-1 sm:grid-cols-2 md:grid-cols-4'),
    'Skeleton grids adapt on mobile screens'
  );

  const heatmap = fs.readFileSync(path.join(__dirname, '../components/ResourceHeatmap.tsx'), 'utf8');
  assert.ok(
    heatmap.includes('grid-cols-2 sm:grid-cols-4'),
    'Heatmap hotspot grid uses 2 columns on mobile'
  );
  assert.ok(
    heatmap.includes('grid-cols-1 sm:grid-cols-2 md:grid-cols-4'),
    'Heatmap footer summary bar wraps to 1 column on extra small screens'
  );
});
