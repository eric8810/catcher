/**
 * Line Router 单元测试 — catcher-web 版
 *
 * 代码与 catcher-http-ts/src/sse/router.ts 完全相同，
 * 本测试验证 catcher-web 打包的 router 行为一致。
 *
 * 用例编号 #1-#24，与设计文档 docs/arch-ts/10-sse.md 一一对应。
 */
import { describe, it, expect } from 'vitest'
import { routeLine } from '../router.js'

describe('Line Router (catcher-web)', () => {
  // ── 1.1 控制行 → Silent ────────────────────────────────────

  describe('控制行 → Silent', () => {
    it('#1 空行 → Silent (事件分隔符)', () => {
      expect(routeLine('')).toEqual({ kind: 'silent' })
    })

    it('#2 `: keepalive` → Silent (心跳)', () => {
      expect(routeLine(': keepalive')).toEqual({ kind: 'silent' })
    })

    it('#3 `: this is a comment` → Silent', () => {
      expect(routeLine(': this is a comment')).toEqual({ kind: 'silent' })
    })

    it('#4 `:` → Silent (最短注释)', () => {
      expect(routeLine(':')).toEqual({ kind: 'silent' })
    })
  })

  // ── 1.2 id: 行 → SetLastEventId ─────────────────────────────

  describe('id: 行 → SetLastEventId', () => {
    it('#5 `id: msg_001` → SetLastEventId("msg_001")', () => {
      expect(routeLine('id: msg_001')).toEqual({ kind: 'setLastEventId', id: 'msg_001' })
    })

    it('#6 `id:msg_002` → SetLastEventId("msg_002") (无空格)', () => {
      expect(routeLine('id:msg_002')).toEqual({ kind: 'setLastEventId', id: 'msg_002' })
    })

    it('#7 `id:  multi  space` → trimStart 只去前导空格', () => {
      expect(routeLine('id:  multi  space')).toEqual({ kind: 'setLastEventId', id: 'multi  space' })
    })

    it('#8 `id:` → SetLastEventId("") (空 id)', () => {
      expect(routeLine('id:')).toEqual({ kind: 'setLastEventId', id: '' })
    })

    it('#9 `id: 42` → SetLastEventId("42") (数字 id)', () => {
      expect(routeLine('id: 42')).toEqual({ kind: 'setLastEventId', id: '42' })
    })
  })

  // ── 1.3 retry: 行 → SetRetry ────────────────────────────────

  describe('retry: 行', () => {
    it('#10 `retry: 5000` → SetRetry(5000)', () => {
      expect(routeLine('retry: 5000')).toEqual({ kind: 'setRetry', ms: 5000 })
    })

    it('#11 `retry:1000` → SetRetry(1000)', () => {
      expect(routeLine('retry:1000')).toEqual({ kind: 'setRetry', ms: 1000 })
    })

    it('#12 `retry: abc` → Yield 原样 (非数字)', () => {
      expect(routeLine('retry: abc')).toEqual({ kind: 'yield', line: 'retry: abc' })
    })

    it('#13 `retry: -1` → Yield 原样 (负数)', () => {
      expect(routeLine('retry: -1')).toEqual({ kind: 'yield', line: 'retry: -1' })
    })

    it('#14 `retry: 0` → SetRetry(0) (零合法，立即重连)', () => {
      expect(routeLine('retry: 0')).toEqual({ kind: 'setRetry', ms: 0 })
    })
  })

  // ── 1.4 内容行 → Yield 原样输出 ─────────────────────────────

  describe('内容行 → Yield 原样输出', () => {
    it('#15 `data: Hello` → Yield 原样', () => {
      expect(routeLine('data: Hello')).toEqual({ kind: 'yield', line: 'data: Hello' })
    })

    it('#16 `data: {"type":"start"}` → Yield 原样 (JSON payload)', () => {
      expect(routeLine('data: {"type":"start"}')).toEqual({ kind: 'yield', line: 'data: {"type":"start"}' })
    })

    it('#17 `data:  world` → Yield 原样 (两个空格保留)', () => {
      expect(routeLine('data:  world')).toEqual({ kind: 'yield', line: 'data:  world' })
    })

    it('#18 `data: [DONE]` → Yield 原样', () => {
      expect(routeLine('data: [DONE]')).toEqual({ kind: 'yield', line: 'data: [DONE]' })
    })

    it('#19 `event: message_start` → Yield 原样', () => {
      expect(routeLine('event: message_start')).toEqual({ kind: 'yield', line: 'event: message_start' })
    })

    it('#20 `data:` → Yield 原样 (空 data)', () => {
      expect(routeLine('data:')).toEqual({ kind: 'yield', line: 'data:' })
    })

    it('#21 `custom: value` → Yield 原样 (非标准前缀)', () => {
      expect(routeLine('custom: value')).toEqual({ kind: 'yield', line: 'custom: value' })
    })

    it('#22 `just text` → Yield 原样 (无前缀行)', () => {
      expect(routeLine('just text')).toEqual({ kind: 'yield', line: 'just text' })
    })

    it('#23 `ID: uppercase` → Yield 原样 (大写不是 id:)', () => {
      expect(routeLine('ID: uppercase')).toEqual({ kind: 'yield', line: 'ID: uppercase' })
    })

    it('#24 ` ` → Yield 原样 (空格非控制前缀)', () => {
      expect(routeLine(' ')).toEqual({ kind: 'yield', line: ' ' })
    })
  })
})
