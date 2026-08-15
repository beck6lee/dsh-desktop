/**
 * dsh-warp-input host half
 *
 * 注册 HTTP 路由 POST /warp/run：
 *   body: { sessionId?: string, command: string, timeoutMs?: number }
 *   resp: { ok, exitCode, timedOut, aborted, stdout, stderr, stdoutTruncated, stderrTruncated } | { ok:false, error }
 *
 * 命令在当前会话工作目录（session.header.cwd）执行，继承会话沙箱策略。
 * 客户端通过同源 fetch 调用（客户端页面由同一 webServer 提供）。
 */

export const name = 'dsh-warp-input'

export const inject = ['webServer', 'shell', 'sessions']

const DEFAULT_TIMEOUT_MS = 60000

function json(res, status, payload) {
  const text = JSON.stringify(payload)
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Content-Length': Buffer.byteLength(text),
  })
  res.end(text)
}

export function apply(ctx) {
  const { webServer, shell, sessions } = ctx

  const handler = async (req, res) => {
    // 仅接受 POST
    if (req.method !== 'POST') {
      json(res, 405, { ok: false, error: 'method not allowed' })
      return
    }
    let body = ''
    req.on('data', (chunk) => {
      body += chunk
      if (body.length > 1_000_000) req.destroy()
    })
    req.on('error', () => {
      json(res, 400, { ok: false, error: 'request aborted' })
    })
    req.on('end', async () => {
      let parsed
      try {
        parsed = JSON.parse(body || '{}')
      } catch (e) {
        json(res, 400, { ok: false, error: 'invalid JSON body' })
        return
      }
      const command = typeof parsed.command === 'string' ? parsed.command.trim() : ''
      if (!command) {
        json(res, 400, { ok: false, error: '空命令' })
        return
      }
      const timeoutMs = Number.isFinite(parsed.timeoutMs)
        ? Math.min(Math.max(parsed.timeoutMs, 1000), 300000)
        : DEFAULT_TIMEOUT_MS
      // 当前会话工作目录
      let cwd
      try {
        const session = sessions.get(parsed.sessionId)
        cwd = session && session.header ? session.header.cwd : undefined
      } catch {
        cwd = undefined
      }
      const request = { command, timeoutMs }
      if (typeof cwd === 'string' && cwd) request.workdir = cwd
      try {
        const spec = shell.resolve(request)
        const result = await shell.run(spec)
        json(res, 200, {
          ok: true,
          exitCode: result.exitCode,
          timedOut: !!result.timedOut,
          aborted: !!result.aborted,
          stdout: (result.stdout && result.stdout.text) || '',
          stderr: (result.stderr && result.stderr.text) || '',
          stdoutTruncated: !!(result.stdout && result.stdout.truncated),
          stderrTruncated: !!(result.stderr && result.stderr.truncated),
        })
      } catch (e) {
        json(res, 500, { ok: false, error: String((e && e.message) || e) })
      }
    })
  }

  return webServer.register({ kind: 'exact', path: '/warp/run', handler })
}
