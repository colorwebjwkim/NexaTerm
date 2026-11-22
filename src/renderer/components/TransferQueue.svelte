<script lang="ts">
  import { files, size, activeTransfer, type Transfer } from '../files'
  let {
    jobs,
    changed,
    reportError,
  }: {
    jobs: Transfer[]
    changed: () => void
    reportError: (value: unknown) => void
  } = $props()
  const labels: Record<string, string> = {
    queued: '대기',
    scanning: '항목 확인',
    running: '전송 중',
    waiting: '동일 이름 확인',
    completed: '완료',
    cancelled: '취소됨',
    failed: '실패',
  }
  async function act(task: () => Promise<unknown>) {
    try {
      await task()
      changed()
    } catch (e) {
      reportError(e)
    }
  }
</script>

{#if jobs.length}<section class="transfer-queue" aria-label="전송 큐">
    <div class="queue-heading">
      <strong>전송 큐</strong><span
        >진행 중 {jobs.filter(activeTransfer).length}</span
      >
    </div>
    <div class="queue-rows">
      {#each jobs as job (job.id)}<article
          class="transfer-job"
          data-status={job.status}
        >
          <div class="job-info">
            <strong>{job.direction === 'upload' ? '↑' : '↓'} {job.name}</strong
            ><small
              >{labels[job.status] || job.status} · {size(job.bytes)} / {size(
                job.total,
              )} · {size(job.speed)}/s · 완료 {job.files} · 건너뜀 {job.skipped}</small
            ><progress
              max={Math.max(job.total, 1)}
              value={Math.min(job.bytes, job.total)}

            ></progress>{#if job.status === 'failed' || job.status === 'cancelled' || (job.status === 'completed' && job.message !== '전송 완료')}<small
                class="queue-error">{job.message}</small
              >{/if}{#if job.conflict}<div class="conflict">
                <span>같은 이름: {job.conflict}</span
                >{#each [{ key: 'skip', label: '건너뛰기' }, { key: 'overwrite', label: '덮어쓰기' }, { key: 'rename', label: '이름 변경' }] as choice (choice.key)}<button
                    onclick={() => act(() => files.decide(job.id, choice.key))}
                    >{choice.label}</button
                  >{/each}
              </div>{/if}
          </div>
          {#if activeTransfer(job)}<button
              onclick={() => act(() => files.cancel(job.id))}>전송 취소</button
            >{:else if ['failed', 'cancelled'].includes(job.status)}<button
              onclick={() => act(() => files.retry(job.id))}>다시 전송</button
            >{/if}
        </article>{/each}
    </div>
  </section>{/if}

<style>
  .transfer-queue {
    height: 170px;
    min-height: 100px;
    max-height: 25%;
    flex-shrink: 0;
    overflow: hidden;
    background: var(--panel);
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    font-size: 11px;
  }
  .queue-heading {
    padding: 7px 12px;
    display: flex;
    gap: 18px;
    color: var(--muted);
  }
  .queue-heading strong {
    color: var(--text);
  }
  .queue-rows {
    overflow: auto;
    min-height: 0;
    overscroll-behavior: contain;
    flex: 1;
  }
  .transfer-job {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 7px 12px;
    border-top: 1px solid var(--line);
  }
  .job-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .job-info strong {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }
  .job-info small {
    color: var(--muted);
  }
  progress {
    height: 4px;
    width: 100%;
    accent-color: var(--accent);
  }
  .transfer-queue button {
    padding: 4px 8px;
    font-size: 11px;
  }
  .queue-error {
    color: var(--danger) !important;
  }
  .conflict {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
  }
  .conflict span {
    color: var(--warning);
    overflow-wrap: anywhere;
  }
</style>
