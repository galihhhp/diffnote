local M = {}

local TERMINAL = "com.superlogical.terminal"
local LABEL = "diffnote"

M.config = {
  key = "cmd+shift+r",
  command = "diffnote",
  ratio = 0.4,
  float = true,
  bounds = { 0, 0, 1, 1 },
}

local function foreground(session_id, block_id)
  local ok, proc = pcall(rex.call, "block.method",
    { session_id = session_id, block_id = block_id, method = "process" })
  local fg = ok and type(proc) == "table" and proc.foreground or {}
  if type(fg) == "table" and not fg.cwd then
    local ok2, res = pcall(rex.call, "block.method",
      { session_id = session_id, block_id = block_id, method = "pwd" })
    if ok2 and type(res) == "table" and type(res.pwd) == "string" and res.pwd ~= "" then
      fg.cwd = (res.pwd:gsub("^file://[^/]*", ""))
    end
  end
  return fg
end

local function blocks_of(session_id)
  local ok, res = pcall(rex.call, "session.list_blocks", { session_id = session_id })
  if not ok or type(res) ~= "table" or type(res.blocks) ~= "table" then return {} end
  return res.blocks
end

local function layers_of(session_id)
  local map = {}
  local ok, view = pcall(rex.call, "session.view", { session_id = session_id })
  if not ok or type(view) ~= "table" then return map end
  for _, window in ipairs(view.windows or {}) do
    for _, layer in ipairs(window.layers or {}) do
      for _, b in ipairs(layer.blocks or {}) do
        if b.block_id then map[b.block_id] = layer end
      end
    end
  end
  return map
end

local function visible(layer)
  if not layer then return false end
  return layer.kind ~= "floating" or layer.state ~= "minimized"
end

local function is_diffnote(b)
  return (b.label or ""):sub(1, #LABEL) == LABEL
end

local function open_new(session_id, anchor, window_id, cwd, label, paired)
  local command = { M.config.command, "--repo", cwd }
  if paired then
    command[#command + 1] = "--session"
    command[#command + 1] = session_id
    command[#command + 1] = "--block"
    command[#command + 1] = paired
  end
  local layout = {
    block = {
      flavor = TERMINAL .. ".shell",
      label = label,
      options = { command = command, cwd = cwd },
    },
  }
  if M.config.float then
    return rex.call("session.new_layer", {
      session_id = session_id,
      window_id = window_id,
      bounds = M.config.bounds,
      focus = true,
      layout = layout,
    })
  end
  return rex.call("session.new_split", {
    session_id = session_id,
    anchor_id = anchor,
    direction = "horizontal",
    side = "after",
    ratio = M.config.ratio,
    focus = true,
    layout = layout,
  })
end

local function hide(session_id, b, layers)
  local layer = layers[b.block_id]
  if layer and layer.kind == "floating" then
    return rex.call("session.set_layer_state", {
      session_id = session_id,
      layer_id = layer.layer_id,
      state = "minimized",
    })
  end
  return rex.call("session.detach_block", { session_id = session_id, block_id = b.block_id })
end

local function show(session_id, b, anchor, layers)
  local layer = layers[b.block_id]
  if layer and layer.kind == "floating" then
    return rex.call("session.set_layer_state", {
      session_id = session_id,
      layer_id = layer.layer_id,
      state = "normal",
      focus = true,
    })
  end
  local res = rex.call("session.move_block", {
    session_id = session_id,
    block_id = b.block_id,
    anchor_block_id = anchor,
    direction = "horizontal",
    side = "after",
    ratio = M.config.ratio,
  })
  rex.call("session.focus_block", { session_id = session_id, block_id = b.block_id })
  return res
end

function M.toggle(ctx)
  local session_id = ctx.session_id or rex.session_id or os.getenv("REX_SESSION")
  local block_id = ctx.block_id or rex.block_id or os.getenv("REX_BLOCK")
  if not session_id then return nil end
  local blocks = blocks_of(session_id)
  local layers = layers_of(session_id)
  local focused
  for _, b in ipairs(blocks) do
    if b.block_id == block_id then focused = b end
  end
  if focused and is_diffnote(focused) then return hide(session_id, focused, layers) end
  local fg = foreground(session_id, block_id)
  local cwd = fg.cwd or ""
  local label = LABEL .. ":" .. tostring(block_id)
  for _, b in ipairs(blocks) do
    if b.label == label then
      if visible(layers[b.block_id]) then return hide(session_id, b, layers) end
      return show(session_id, b, block_id, layers)
    end
  end
  return open_new(session_id, block_id, focused and focused.window_id, cwd, label, block_id)
end

M.open = M.toggle

function M.setup(opts)
  for k, v in pairs(opts or {}) do M.config[k] = v end
  if M.config.command == "diffnote" or M.config.command == "" then
    local result = rex.exec({ "/bin/zsh", "-lic", "command -v diffnote" })
    local resolved = result and result.ok and result.stdout
      and result.stdout:match("[^\n]+")
    if resolved and resolved ~= "" then
      M.config.command = resolved
    else
      M.config.command = os.getenv("HOME") .. "/.local/bin/diffnote"
    end
  end
  rex.action({
    name = "diffnote_open",
    title = "Show or hide diffnote",
    run = function(ctx) return M.toggle(ctx or {}) end,
  })
  if M.config.key then
    rex.bind(M.config.key, function(ctx)
      M.toggle(ctx or {})
      return true
    end)
  end
end

return M
