Header = {
	-- TODO: remove these two constants
	LEFT = 0,
	RIGHT = 1,

	_id = "header",
	_inc = 1000,
	_left = {
		{ "cwd", id = 1, order = 1000 },
		{ "mode", id = 2, order = 2000 },
		{ "size", id = 3, order = 3000 },
	},
	_right = {
		{ "perm", id = 4, order = 1000 },
		{ "percent", id = 5, order = 2000 },
		{ "position", id = 6, order = 3000 },
		{ "count", id = 7, order = 4000 },
	},
	-- Click targets for breadcrumb segments: { x0, x1, url }
	_cwd_hits = {},
}

function Header:new(area, tab)
	return setmetatable({
		_area = area,
		_tab = tab,
		_current = tab.current,
		_cwd_hits = {},
	}, { __index = self })
end

--- Display width of a span/string (Span has no :width(); Line does).
local function text_width(s)
	return ui.Line({ s }):width()
end

--- Build the parent chain root → current for breadcrumb navigation.
function Header:path_chain()
	local chain = {}
	local ok, u = pcall(function()
		return self._current.cwd
	end)
	if not ok or not u then
		return chain
	end
	local guard = 0
	while u and guard < 48 do
		chain[#chain + 1] = u
		local p_ok, p = pcall(function()
			return u.parent
		end)
		if not p_ok or not p then
			break
		end
		-- Stop when parent is the same node (scheme root / drive root).
		local same = false
		pcall(function()
			same = tostring(p) == tostring(u)
		end)
		if same then
			break
		end
		u = p
		guard = guard + 1
	end
	-- Reverse so root is first.
	local ordered = {}
	for i = #chain, 1, -1 do
		ordered[#ordered + 1] = chain[i]
	end
	return ordered
end

function Header:segment_label(url, is_root)
	if is_root then
		return ya.readable_path(tostring(url))
	end
	local name_ok, name = pcall(function()
		return url.name
	end)
	if name_ok and name and tostring(name) ~= "" then
		return tostring(name)
	end
	local urn_ok, urn = pcall(function()
		return url.urn
	end)
	if urn_ok and urn then
		local s = tostring(urn)
		if s ~= "" then
			return s
		end
	end
	return ya.readable_path(tostring(url))
end

function Header:cwd()
	local max = (self._area.w or 0) - (self._right_width or 0)
	if max <= 0 then
		self._cwd_hits = {}
		return ""
	end

	local style = th.mgr.cwd
	local chain = self:path_chain()

	-- Fallback to classic single truncated path if chain fails.
	if #chain == 0 then
		local s = ya.readable_path(tostring(self._current.cwd)) .. self:flags()
		self._cwd_hits = {
			{
				x0 = self._area.x,
				x1 = self._area.x + math.min(max, text_width(s)),
				url = self._current.cwd,
			},
		}
		return ui.Span(ui.truncate(s, { max = max, rtl = true })):style(style)
	end

	local labels = {}
	for i, url in ipairs(chain) do
		labels[i] = self:segment_label(url, i == 1)
	end

	local sep = " › "
	local sep_w = text_width(sep)

	local function crumb_width(from_idx, with_ellipsis)
		local w = 0
		if with_ellipsis and from_idx > 2 then
			w = text_width(labels[1]) + sep_w + text_width("…") + sep_w
			for i = from_idx, #labels do
				if i > from_idx then
					w = w + sep_w
				end
				w = w + text_width(labels[i])
			end
		else
			for i = from_idx, #labels do
				if i > from_idx then
					w = w + sep_w
				end
				w = w + text_width(labels[i])
			end
		end
		return w + text_width(self:flags())
	end

	local start_i = 1
	local use_ellipsis = false
	if crumb_width(1, false) > max and #labels > 2 then
		for keep = #labels, 2, -1 do
			if crumb_width(keep, true) <= max then
				start_i = keep
				use_ellipsis = true
				break
			end
		end
		-- If nothing fits with ellipsis, fall back to truncated full path.
		if not use_ellipsis then
			local s = ya.readable_path(tostring(self._current.cwd)) .. self:flags()
			self._cwd_hits = {
				{
					x0 = self._area.x,
					x1 = self._area.x + math.min(max, text_width(s)),
					url = self._current.cwd,
				},
			}
			return ui.Span(ui.truncate(s, { max = max, rtl = true })):style(style)
		end
	end

	local spans = {}
	local hits = {}
	local col = 0

	local function push_text(text, url, sty)
		local span = ui.Span(text):style(sty or style)
		local w = text_width(text)
		if url then
			hits[#hits + 1] = {
				x0 = self._area.x + col,
				x1 = self._area.x + col + w,
				url = url,
			}
		end
		spans[#spans + 1] = span
		col = col + w
	end

	if use_ellipsis and start_i > 2 then
		push_text(labels[1], chain[1], style)
		push_text(sep, nil, style)
		push_text("…", nil, style)
		push_text(sep, nil, style)
		for i = start_i, #labels do
			if i > start_i then
				push_text(sep, nil, style)
			end
			push_text(labels[i], chain[i], style)
		end
	else
		for i, url in ipairs(chain) do
			if i > 1 then
				push_text(sep, nil, style)
			end
			push_text(labels[i], url, style)
		end
	end

	local flags = self:flags()
	if flags ~= "" then
		push_text(flags, nil, style)
	end

	-- Still too wide → classic rtl truncate of full path.
	if col > max then
		local s = ya.readable_path(tostring(self._current.cwd)) .. self:flags()
		self._cwd_hits = {
			{
				x0 = self._area.x,
				x1 = self._area.x + math.min(max, text_width(s)),
				url = self._current.cwd,
			},
		}
		return ui.Span(ui.truncate(s, { max = max, rtl = true })):style(style)
	end

	self._cwd_hits = hits
	return ui.Line(spans)
end

function Header:flags()
	local cwd = self._current.cwd
	local filter = self._current.files.filter
	local finder = self._tab.finder

	local t = {}
	if cwd.is_search then
		t[#t + 1] = string.format("search: %s", cwd.domain)
	end
	if filter then
		t[#t + 1] = string.format("filter: %s", filter)
	end
	if finder then
		t[#t + 1] = string.format("find: %s", finder)
	end
	return #t == 0 and "" or " (" .. table.concat(t, ", ") .. ")"
end

function Header:count()
	local selected = #self._tab.selected
	local yanked = selected > 0 and 0 or #cx.yanked

	local span
	if selected > 0 then
		span = ui.Span(" " .. selected .. " "):style(th.mgr.count_selected)
	elseif yanked <= 0 then
		return ""
	elseif cx.yanked.is_cut then
		span = ui.Span(" " .. yanked .. " "):style(th.mgr.count_cut)
	else
		span = ui.Span(" " .. yanked .. " "):style(th.mgr.count_copied)
	end

	return ui.Line { span, " " }
end

function Header:style()
	local m = th.mode
	if self._tab.mode.is_select then
		return { main = m.select_main, alt = m.select_alt }
	elseif self._tab.mode.is_unset then
		return { main = m.unset_main, alt = m.unset_alt }
	else
		return { main = m.normal_main, alt = m.normal_alt }
	end
end

function Header:mode()
	local mode = tostring(self._tab.mode):sub(1, 3):upper()

	local style = self:style()
	return ui.Line {
		ui.Span(" "):fg("reset"),
		ui.Span(th.status.sep_left.open):fg(style.main:bg()):bg("reset"),
		ui.Span(" " .. mode .. " "):style(style.main),
		ui.Span(th.status.sep_left.close):fg(style.main:bg()):bg(style.alt:bg()),
	}
end

function Header:size()
	local h = self._current.hovered
	local size = h and (h:size() or h.cha.len) or 0

	local style = self:style()
	return ui.Line {
		ui.Span(" " .. ya.readable_size(size) .. " "):style(style.alt),
		ui.Span(th.status.sep_left.close):fg(style.alt:bg()),
	}
end

function Header:perm()
	local h = self._current.hovered
	if not h then
		return ""
	end

	local perm = h.cha:perm()
	if not perm then
		return ""
	end

	local spans = {}
	for i = 1, #perm do
		local c = perm:sub(i, i)
		local style = th.status.perm_type
		if c == "-" or c == "?" then
			style = th.status.perm_sep
		elseif c == "r" then
			style = th.status.perm_read
		elseif c == "w" then
			style = th.status.perm_write
		elseif c == "x" or c == "s" or c == "S" or c == "t" or c == "T" then
			style = th.status.perm_exec
		end
		spans[i] = ui.Span(c):style(style)
	end
	return ui.Line(spans)
end

function Header:percent()
	local percent = 0
	local cursor = self._current.cursor
	local length = #self._current.files
	if cursor ~= 0 and length ~= 0 then
		percent = math.floor((cursor + 1) * 100 / length)
	end

	if percent == 0 then
		percent = " Top "
	elseif percent == 100 then
		percent = " Bot "
	else
		percent = string.format(" %2d%% ", percent)
	end

	local style = self:style()
	return ui.Line {
		ui.Span(" " .. th.status.sep_right.open):fg(style.alt:bg()),
		ui.Span(percent):style(style.alt),
	}
end

function Header:position()
	local cursor = self._current.cursor
	local length = #self._current.files

	local style = self:style()
	return ui.Line {
		ui.Span(th.status.sep_right.open):fg(style.main:bg()):bg(style.alt:bg()),
		ui.Span(string.format(" %2d/%-2d ", math.min(cursor + 1, length), length)):style(style.main),
		ui.Span(th.status.sep_right.close):fg(style.main:bg()):bg("reset"),
	}
end

function Header:reflow() return { self } end

function Header:redraw()
	local right = self:children_redraw(self.RIGHT)
	self._right_width = right:width()

	local left = self:children_redraw(self.LEFT)

	return {
		ui.Line(left):area(self._area),
		ui.Line(right):area(self._area):align(ui.Align.RIGHT),
	}
end

-- Mouse events
function Header:click(event, up)
	if up then
		return
	end

	-- Right / middle click on the header → go up one directory.
	if event.is_right or event.is_middle then
		ya.emit("leave", {})
		return
	end

	if not event.is_left then
		return
	end

	local hits = self._cwd_hits or {}
	-- Prefer the deepest (rightmost) segment under the cursor.
	for i = #hits, 1, -1 do
		local h = hits[i]
		if event.x >= h.x0 and event.x < h.x1 and h.url then
			if tostring(h.url) == tostring(self._current.cwd) then
				return
			end
			ya.emit("cd", { h.url })
			return
		end
	end

	-- Click on empty header chrome: go up one level.
	ya.emit("leave", {})
end

function Header:scroll(event, step)
	ya.emit("arrow", { step })
end

function Header:touch(event, step) end

-- Children
function Header:children_add(fn, order, side)
	self._inc = self._inc + 1
	local children = side == self.RIGHT and self._right or self._left

	children[#children + 1] = { fn, id = self._inc, order = order }
	table.sort(children, function(a, b) return a.order < b.order end)

	return self._inc
end

function Header:children_remove(id, side)
	local children = side == self.RIGHT and self._right or self._left
	for i, child in ipairs(children) do
		if child.id == id then
			table.remove(children, i)
			break
		end
	end
end

function Header:children_redraw(side)
	local lines = {}
	for _, c in ipairs(side == self.RIGHT and self._right or self._left) do
		lines[#lines + 1] = (type(c[1]) == "string" and self[c[1]] or c[1])(self)
	end
	return ui.Line(lines)
end
