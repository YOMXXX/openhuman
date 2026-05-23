import { describe, expect, it } from 'vitest';

import type { GraphNode } from '../../utils/tauriCommands';
import { MEMORY_CONTENT_WORKSPACE_PATH, summaryWorkspacePath } from './memoryWorkspacePaths';

function summaryNode(overrides: Partial<GraphNode> = {}): GraphNode {
  return {
    kind: 'summary',
    id: 'summary-1',
    label: 'Summary',
    tree_id: 'tree-1',
    tree_kind: 'topic',
    tree_scope: 'Team_Name',
    level: 1,
    parent_id: null,
    child_count: 0,
    file_basename: 'summary-file',
    ...overrides,
  };
}

describe('memoryWorkspacePaths', () => {
  it('returns null for graph nodes that cannot resolve to a summary file', () => {
    expect(summaryWorkspacePath({ kind: 'chunk', id: 'c1', label: 'Chunk' })).toBeNull();
    expect(summaryWorkspacePath(summaryNode({ file_basename: undefined }))).toBeNull();
  });

  it('preserves internal underscores while trimming leading/trailing separators', () => {
    expect(
      summaryWorkspacePath(summaryNode({ tree_kind: 'source', tree_scope: '__Team_Name__' }))
    ).toBe(`${MEMORY_CONTENT_WORKSPACE_PATH}/wiki/summaries/source-team_name/L1/summary-file.md`);
  });

  it('uses unknown-date for global summaries with missing or invalid timestamps', () => {
    expect(
      summaryWorkspacePath(summaryNode({ tree_kind: 'global', time_range_start_ms: undefined }))
    ).toBe(
      `${MEMORY_CONTENT_WORKSPACE_PATH}/wiki/summaries/global-unknown-date/L1/summary-file.md`
    );
    expect(
      summaryWorkspacePath(
        summaryNode({ tree_kind: 'global', time_range_start_ms: Number.MAX_SAFE_INTEGER })
      )
    ).toBe(
      `${MEMORY_CONTENT_WORKSPACE_PATH}/wiki/summaries/global-unknown-date/L1/summary-file.md`
    );
  });
});
