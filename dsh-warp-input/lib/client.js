/**
 * dsh-warp-input client half
 *
 * 接管 conversation.composer（chain 槽）：
 *  - 输入以 `$` 开头 → 命令模式：POST /warp/run 在会话目录执行，内联显示输出
 *  - 否则 → 正常对话（inputActions.submit()，斜杠命令等原有行为保留）
 *  - hero 阶段（无会话）或存在待处理交互（问题/审批）时让位
 */
window.__ModuleLoader__.load({
  id: 'dsh-warp-input',
  factory: (require) => {
    var module = { exports: {} }
    var exports = module.exports
    var React = require('react')

    var inject = ['slots']

    // 命令历史：按 sessionId 记忆（页面会话内跨重挂载存活），↑/↓ 在命令模式浏览
    var HISTORY = Object.create(null)
    var HISTORY_CAP = 50
    function pushHistory(sessionId, cmd) {
      var list = HISTORY[sessionId] || (HISTORY[sessionId] = [])
      if (list[list.length - 1] !== cmd) {
        list.push(cmd)
        if (list.length > HISTORY_CAP) list.splice(0, list.length - HISTORY_CAP)
      }
    }

    function apply(ctx) {
      var slots = ctx.slots

      var css = [
        '.warp-composer { display:flex; flex-direction:column; gap:8px; width:100%; max-width:var(--dsh-composer-card-max-width, 780px); margin:0 auto; padding:10px 12px; border:1px solid var(--dsw-alias-border-l2); border-radius:16px; background:var(--dsw-alias-bg-base); }',
        '.warp-textarea { width:100%; min-height:48px; max-height:200px; resize:none; border:none; outline:none; background:transparent; color:var(--dsw-alias-label-primary); font:14px/20px var(--ds-font-family, -apple-system); }',
        '.warp-textarea:disabled { opacity:.6; }',
        '.warp-row { display:flex; align-items:center; gap:8px; }',
        '.warp-badge { font:11px/16px var(--ds-font-family-code, monospace); color:var(--dsw-alias-state-business-primary); background:color-mix(in srgb, var(--dsw-alias-state-business-primary) 12%, transparent); border-radius:6px; padding:1px 8px; }',
        '.warp-badge-chat { color:var(--dsw-alias-label-tertiary); background:var(--dsw-alias-bg-module-platform); }',
        '.warp-send { margin-left:auto; border:none; border-radius:10px; background:var(--dsw-alias-state-business-primary); color:#fff; font-size:13px; padding:6px 14px; cursor:pointer; }',
        '.warp-send:disabled { opacity:.5; cursor:default; }',
        '.warp-out { display:flex; flex-direction:column; gap:6px; border-top:1px solid var(--dsw-alias-border-l1); padding-top:8px; max-height:240px; overflow:auto; }',
        '.warp-cmd { color:var(--dsw-alias-label-secondary); font:12px/18px var(--ds-font-family-code, monospace); white-space:pre-wrap; }',
        '.warp-pre { margin:0; color:var(--dsw-alias-label-primary); background:var(--dsw-alias-bg-layer-1); border-radius:8px; padding:8px 10px; font:12px/18px var(--ds-font-family-code, monospace); white-space:pre-wrap; word-break:break-all; }',
        '.warp-pre-err { color:var(--dsw-alias-state-error-primary); }',
        '.warp-pill { font:11px/16px var(--ds-font-family-code, monospace); border-radius:999px; padding:1px 8px; }',
        '.warp-pill-ok { color:var(--dsw-alias-state-success-primary); background:color-mix(in srgb, var(--dsw-alias-state-success-primary) 14%, transparent); }',
        '.warp-pill-err { color:var(--dsw-alias-state-error-primary); background:color-mix(in srgb, var(--dsw-alias-state-error-primary) 14%, transparent); }',
        '.warp-pill-run { color:var(--dsw-alias-state-warn-label); background:color-mix(in srgb, var(--dsw-alias-state-warn-label) 14%, transparent); }',
        '.warp-hint { color:var(--dsw-alias-label-caption); font-size:11px; }',
      ].join('\n')

      ctx.effect(() => {
        var tag = document.createElement('style')
        tag.dataset.plugin = 'dsh-warp-input'
        tag.textContent = css
        document.head.appendChild(tag)
        return () => tag.remove()
      })

      function runCommand(draft, sessionId, setView) {
        var cmd = draft.trim().slice(1).trim()
        if (!cmd) return
        pushHistory(sessionId, cmd)
        setView({ running: true, result: null })
        fetch('/warp/run', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ sessionId: sessionId, command: cmd, timeoutMs: 60000 }),
        })
          .then(function (r) { return r.json() })
          .then(function (res) {
            if (res && res.ok === true) {
              setView({ running: false, result: { kind: 'result', cmd: cmd, res: res } })
            } else {
              setView({ running: false, result: { kind: 'error', cmd: cmd, error: (res && res.error) || '执行失败' } })
            }
          })
          .catch(function (e) {
            setView({ running: false, result: { kind: 'error', cmd: cmd, error: String((e && e.message) || e) } })
          })
      }

      function WarpComposer(props) {
        var useInput = props.useInput
        var inputActions = props.inputActions
        var sessionId = props.sessionId
        var input = typeof useInput === 'function' ? useInput(function (s) { return s }) : undefined
        var draft = (input && input.draft) || ''
        var isCommand = draft.trim().indexOf('$') === 0
        var state = React.useState({ running: false, result: null })
        var view = state[0]
        var setView = state[1]
        var running = view.running
        var result = view.result
        // 命令历史浏览：idx=-1 表示未在浏览；pending 保存浏览前的草稿
        var idxState = React.useState(-1)
        var idx = idxState[0]
        var setIdx = idxState[1]
        var pendingState = React.useState('')
        var pending = pendingState[0]
        var setPending = pendingState[1]
        var historyList = (HISTORY[sessionId] || [])

        function send() {
          if (isCommand) {
            runCommand(draft, sessionId, setView)
            if (typeof inputActions.setDraft === 'function') inputActions.setDraft('')
            if (idx !== -1) setIdx(-1)
            if (pending) setPending('')
          } else if (typeof inputActions.submit === 'function') {
            inputActions.submit()
          }
        }

        function onKeyDown(e) {
          if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault()
            send()
            return
          }
          // 命令模式下 ↑/↓ 浏览历史；非命令模式保留 textarea 默认行为
          if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && isCommand && !e.nativeEvent.isComposing) {
            if (historyList.length === 0) return
            e.preventDefault()
            if (e.key === 'ArrowUp') {
              if (idx === -1) setPending(draft)
              var up = idx === -1 ? historyList.length - 1 : Math.max(0, idx - 1)
              setIdx(up)
              if (typeof inputActions.setDraft === 'function') inputActions.setDraft('$ ' + historyList[up])
            } else {
              if (idx === -1) return
              var down = idx + 1
              if (down >= historyList.length) {
                setIdx(-1)
                if (typeof inputActions.setDraft === 'function') inputActions.setDraft(pending)
              } else {
                setIdx(down)
                if (typeof inputActions.setDraft === 'function') inputActions.setDraft('$ ' + historyList[down])
              }
            }
          }
        }

        function renderResult() {
          if (!result) return null
          var children = [
            React.createElement('span', { className: 'warp-cmd', key: 'cmd' }, '$ ' + result.cmd),
          ]
          if (result.kind === 'error') {
            children.push(React.createElement('pre', { className: 'warp-pre warp-pre-err', key: 'err' }, result.error))
            return React.createElement('div', { className: 'warp-out', key: 'out' }, children)
          }
          var res = result.res
          if (res.stdout) children.push(React.createElement('pre', { className: 'warp-pre', key: 'out' }, res.stdout))
          if (res.stderr) children.push(React.createElement('pre', { className: 'warp-pre warp-pre-err', key: 'err' }, res.stderr))
          var pill = res.timedOut
            ? { cls: 'warp-pill-run', text: 'TIMEOUT' }
            : res.exitCode === 0
              ? { cls: 'warp-pill-ok', text: 'exit 0' }
              : { cls: 'warp-pill-err', text: 'exit ' + String(res.exitCode) }
          children.push(React.createElement('span', { className: 'warp-pill ' + pill.cls, key: 'pill' }, pill.text))
          if (res.stdoutTruncated || res.stderrTruncated) {
            children.push(React.createElement('span', { className: 'warp-hint', key: 'trunc' }, '（输出过长已截断）'))
          }
          return React.createElement('div', { className: 'warp-out', key: 'out' }, children)
        }

        return React.createElement('div', { className: 'warp-composer' },
          React.createElement('textarea', {
            className: 'warp-textarea',
            value: draft,
            placeholder: isCommand ? '在会话目录执行命令…' : '对话消息；以 $ 开头为命令（如 $ ls -la）',
            onChange: function (e) {
              // 手动编辑时退出历史浏览
              if (idx !== -1) {
                setIdx(-1)
                setPending('')
              }
              if (typeof inputActions.setDraft === 'function') inputActions.setDraft(e.target.value)
            },
            onKeyDown: onKeyDown,
            disabled: running,
          }),
          React.createElement('div', { className: 'warp-row' },
            React.createElement('span', { className: isCommand ? 'warp-badge' : 'warp-badge warp-badge-chat' },
              isCommand ? '$ 命令' : '对话'),
            (isCommand && historyList.length > 0)
              ? React.createElement('span', { className: 'warp-hint' }, '↑/↓ 历史')
              : null,
            running
              ? React.createElement('span', { className: 'warp-pill warp-pill-run' }, '运行中…')
              : null,
            React.createElement('button', { className: 'warp-send', onClick: send, disabled: running },
              isCommand ? '执行' : '发送'),
          ),
          renderResult(),
        )
      }

      slots.inject('conversation.composer', function () {
        return slots.register({
          name: 'conversation.composer',
          select: function (owner) {
            // 无会话（hero 阶段）或存在待处理交互（问题/审批接管）时让位
            if (!owner || !owner.session) return null
            var interactions = owner.interactions
            if (interactions && interactions.length > 0) return null
            return { kind: 'warp-command' }
          },
        }, function (props) {
          return React.createElement(WarpComposer, props)
        })
      })
    }

    exports.apply = apply
    exports.inject = inject
    return module.exports
  },
})
