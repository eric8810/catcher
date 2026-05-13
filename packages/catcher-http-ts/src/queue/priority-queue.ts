import PQueue from 'p-queue'
import type { PriorityQueueOptions } from '@eric8810/catcher-core'

/**
 * Create a priority queue for request scheduling.
 * Lower number = higher priority (0 = highest).
 */
export function createPriorityQueue(options: PriorityQueueOptions = {}): PQueue {
  const { concurrency = 10, timeout } = options

  return new PQueue({
    concurrency,
    timeout,
    throwOnTimeout: true,
  })
}

/**
 * Add a task with priority.
 * priority: 0 = highest (message sending), 5 = lowest (prefetch).
 */
export function enqueueWithPriority(
  queue: PQueue,
  priority: number,
  fn: () => Promise<any>,
): Promise<any> {
  return queue.add(fn, { priority })
}
