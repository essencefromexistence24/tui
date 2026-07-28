// Menu mouse interaction handling
use super::menu_data::Menu;

impl Menu {
	pub fn handle_mouse(&mut self, x: u16, y: u16, is_click: bool) -> bool {
		// Check if mouse is within menu area
		if x < self.menu_area.x
			|| x >= self.menu_area.x + self.menu_area.width
			|| y < self.menu_area.y
			|| y >= self.menu_area.y + self.menu_area.height
		{
			if self.hovered_menu_item.is_some() {
				self.hovered_menu_item = None;
				return true;
			}
			return false;
		}

		// Check which menu item is being hovered/clicked
		for (visible_idx, area) in self.menu_item_areas.iter().enumerate() {
			if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
				let actual_idx = self.scroll_offset + visible_idx;

				if is_click {
					self.selected_menu_item = actual_idx;
					return true;
				} else {
					if self.hovered_menu_item != Some(actual_idx) {
						self.hovered_menu_item = Some(actual_idx);
						return true;
					}
					return false;
				}
			}
		}

		if self.hovered_menu_item.is_some() {
			self.hovered_menu_item = None;
			return true;
		}
		false
	}
}
