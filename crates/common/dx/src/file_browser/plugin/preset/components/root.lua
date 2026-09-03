Root = {
	_id = "root",
	_drag_start = ui.Rect {},
}

function Root:new(area)
	local me = setmetatable({ _area = area }, { __index = self })
	me:layout()
	me:build()
	return me
end

function Root:layout()
	self._chunks = ui.Layout()
		:direction(ui.Layout.VERTICAL)
		:constraints({
			ui.Constraint.Length(1),
			ui.Constraint.Length(Tabs.height()),
			ui.Constraint.Fill(1),
		})
		:split(self._area)
end

function Root:build()
	-- Create border bars
	self._base = {
		-- Top border
		ui.Bar(ui.Edge.TOP):area(self._area):symbol("─"):style(th.pick.border),
		-- Bottom border
		ui.Bar(ui.Edge.BOTTOM):area(self._area):symbol("─"):style(th.pick.border),
		-- Left border
		ui.Bar(ui.Edge.LEFT):area(self._area):symbol("│"):style(th.pick.border),
		-- Right border
		ui.Bar(ui.Edge.RIGHT):area(self._area):symbol("│"):style(th.pick.border),
	}
	
	self._children = {
		Header:new(self._chunks[1], cx.active),
		Tabs:new(self._chunks[2]),
		Tab:new(self._chunks[3], cx.active),
		Modal:new(self._area),
	}
end

function Root:reflow()
	local components = { self }
	for _, child in ipairs(self._children) do
		components = ya.list_merge(components, child:reflow())
	end
	return components
end

function Root:redraw()
	local elements = self._base or {}
	for _, child in ipairs(self._children) do
		elements = ya.list_merge(elements, ui.redraw(child))
	end
	return elements
end

-- Mouse events
function Root:click(event, up)
	if tostring(cx.layer) ~= "mgr" then
		return
	end
	local c = ya.child_at(ui.Rect { x = event.x, y = event.y }, self:reflow())
	return c and c:click(event, up)
end

function Root:scroll(event, step)
	if tostring(cx.layer) ~= "mgr" then
		return
	end
	local c = ya.child_at(ui.Rect { x = event.x, y = event.y }, self:reflow())
	return c and c:scroll(event, step)
end

function Root:touch(event, step)
	if tostring(cx.layer) ~= "mgr" then
		return
	end
	local c = ya.child_at(ui.Rect { x = event.x, y = event.y }, self:reflow())
	return c and c:touch(event, step)
end

function Root:move(event) end

function Root:drag(event) end
