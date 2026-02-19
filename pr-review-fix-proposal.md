# Proposed fix for intermittent PR Review failures

## Problem

The `PR Review` workflow intermittently fails with:

```
"type": "result",
"subtype": "success",
"is_error": true,
"num_turns": 1,
"total_cost_usd": 0.000572
```

This means the Anthropic API returned an error on the first call. The actual
error message is hidden because `show_full_output` is `false`. The CLI then
exits with code 1 (verified in `cli.js`: `O3(N?.type==="result"&&N?.is_error?1:0)`),
and the SDK wrapper throws a generic "Claude Code process exited with code 1" error.

The failures are intermittent: PR #893 succeeded 3 times then failed 2 times.

## Root cause gap

In `anthropics/claude-code-action`, `run-claude-sdk.ts` line 136 checks:

```typescript
const isSuccess = resultMessage.subtype === "success";
```

But the CLI can emit `subtype: "success"` with `is_error: true` (meaning the
API returned an error that was stored as the final assistant message). The action
treats this as success and never logs the error content. Then the CLI exits
with code 1, the SDK throws, and the action reports a useless generic error.

## Proposed change for `foxglove/actions`

File: `.github/workflows/review.yml`

The review step should retry on failure, since the errors are intermittent API
errors. The second attempt usually succeeds (as shown by the PR #893 history).

```yaml
      - name: Run Claude review
        id: review
        uses: anthropics/claude-code-action@v1
        continue-on-error: true
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          track_progress: false
          use_sticky_comment: false
          allowed_bots: cursor[bot]
          prompt: |
            ...
          claude_args: |
            --model claude-opus-4-6
            --allowedTools "..."

      - name: Retry Claude review on failure
        if: steps.review.outcome == 'failure'
        uses: anthropics/claude-code-action@v1
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          track_progress: false
          use_sticky_comment: false
          allowed_bots: cursor[bot]
          prompt: |
            ...
          claude_args: |
            --model claude-opus-4-6
            --allowedTools "..."
```

This adds a second attempt when the first fails, handling the intermittent API
errors without exposing sensitive output.

### Alternative: surface the error for diagnosis

If you want to see the actual API error first (to potentially report it to
Anthropic), temporarily add `show_full_output: 'true'` to the action step:

```yaml
      - name: Run Claude review
        uses: anthropics/claude-code-action@v1
        with:
          show_full_output: 'true'  # TEMPORARY - remove after diagnosis
          ...
```

WARNING: this exposes all Claude output in the public GitHub Actions logs,
which may include file contents from the repo.
