#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DxToolActionKind {
	StageInInput,
	CopyToClipboard,
	ConfirmThenStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DxToolAction {
	pub command: &'static str,
	pub kind: DxToolActionKind,
	pub confirmation: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DxToolEntry {
	pub label: &'static str,
	pub command: &'static str,
	pub action_kind: DxToolActionKind,
	pub confirmation: Option<&'static str>,
}

pub(crate) const DX_TOOLS_SUBMENU_INDEX: usize = 25;

struct CatalogEntry {
	name: &'static str,
	menu_label: &'static str,
	primary_command: &'static str,
	action_kind: DxToolActionKind,
}

const CATALOG: &[CatalogEntry] = &[
	CatalogEntry {
		name: "py",
		menu_label: "1. Py",
		primary_command: "dx py status",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "forge",
		menu_label: "2. Forge",
		primary_command: "dx forge status",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "js",
		menu_label: "3. JS",
		primary_command: "dx js contracts",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "check",
		menu_label: "4. Check",
		primary_command: "dx check --json",
		action_kind: DxToolActionKind::ConfirmThenStage,
	},
	CatalogEntry {
		name: "style",
		menu_label: "5. Style",
		primary_command: "dx style status",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "www",
		menu_label: "6. WWW",
		primary_command: "dx www build",
		action_kind: DxToolActionKind::ConfirmThenStage,
	},
	CatalogEntry {
		name: "native",
		menu_label: "7. Native",
		primary_command: "dx native status",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "build",
		menu_label: "8. Build",
		primary_command: "dx build lighthouse --contract --json",
		action_kind: DxToolActionKind::ConfirmThenStage,
	},
	CatalogEntry {
		name: "icon",
		menu_label: "9. Icon",
		primary_command: "dx icon search home --pack lucide",
		action_kind: DxToolActionKind::CopyToClipboard,
	},
	CatalogEntry {
		name: "media",
		menu_label: "10. Media",
		primary_command: "dx media status",
		action_kind: DxToolActionKind::CopyToClipboard,
	},
	CatalogEntry {
		name: "serializer",
		menu_label: "11. Serializer",
		primary_command: "dx serializer dx",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "update",
		menu_label: "12. Update",
		primary_command: "dx update plan",
		action_kind: DxToolActionKind::ConfirmThenStage,
	},
	CatalogEntry {
		name: "doctor",
		menu_label: "13. Doctor",
		primary_command: "dx doctor --json",
		action_kind: DxToolActionKind::StageInInput,
	},
	CatalogEntry {
		name: "status",
		menu_label: "14. Status",
		primary_command: "dx status --json",
		action_kind: DxToolActionKind::StageInInput,
	},
];

pub(crate) fn entries() -> Vec<DxToolEntry> {
	CATALOG
		.iter()
		.map(|tool| {
			let confirmation = match tool.action_kind {
				DxToolActionKind::ConfirmThenStage => confirmation_text(tool.name),
				_ => None,
			};
			DxToolEntry {
				label: tool.menu_label,
				command: tool.primary_command,
				action_kind: tool.action_kind,
				confirmation,
			}
		})
		.collect()
}

pub fn get_submenu() -> Vec<(&'static str, &'static str)> {
	entries().into_iter().map(|entry| (entry.label, entry.command)).collect()
}

pub(crate) fn action_for_menu_item(
	selected_item: usize,
	opened_directly: bool,
) -> Option<DxToolAction> {
	let entry_index = if opened_directly { selected_item } else { selected_item.checked_sub(1)? };
	let entries = entries();
	entries.get(entry_index).map(|entry| DxToolAction {
		command: entry.command,
		kind: entry.action_kind,
		confirmation: entry.confirmation,
	})
}

fn confirmation_text(tool_name: &str) -> Option<&'static str> {
	match tool_name {
		"check" => Some("Check can inspect project state. Press Enter/Y to stage it."),
		"www" => Some("WWW build can be heavy. Press Enter/Y to stage it."),
		"build" => Some("Build Lighthouse can be heavy. Press Enter/Y to stage it."),
		"update" => Some("Update planning reads release metadata. Press Enter/Y to stage it."),
		_ => Some("This DX command needs confirmation. Press Enter/Y to stage it."),
	}
}

#[cfg(test)]
mod tests {
	use super::{CATALOG, DxToolActionKind, action_for_menu_item, entries, get_submenu};

	#[test]
	fn submenu_rows_derive_from_public_catalog() {
		let rows = get_submenu();

		assert_eq!(rows.len(), CATALOG.len());
		assert_eq!(rows[0], ("1. Py", "dx py status"));
		assert_eq!(rows[9], ("10. Media", "dx media status"));
		assert_eq!(rows[13], ("14. Status", "dx status --json"));
		for (entry, tool) in entries().into_iter().zip(CATALOG) {
			assert_eq!(entry.label, tool.menu_label);
			assert_eq!(entry.command, tool.primary_command);
		}
	}

	#[test]
	fn action_mapping_accounts_for_back_row() {
		assert_eq!(action_for_menu_item(0, true).expect("direct first action").command, "dx py status");
		assert!(action_for_menu_item(0, false).is_none());
		assert_eq!(
			action_for_menu_item(1, false).expect("submenu first action").command,
			"dx py status"
		);
	}

	#[test]
	fn heavy_dx_tool_commands_require_confirmation() {
		let heavy = entries()
			.into_iter()
			.filter(|entry| entry.action_kind == DxToolActionKind::ConfirmThenStage)
			.map(|entry| entry.command)
			.collect::<Vec<_>>();

		assert_eq!(
			heavy,
			[
				"dx check --json",
				"dx www build",
				"dx build lighthouse --contract --json",
				"dx update plan",
			]
		);
	}

	#[test]
	fn media_dx_tool_action_stays_copy_only() {
		let media = action_for_menu_item(9, true).expect("direct media action");

		assert_eq!(media.command, "dx media status");
		assert_eq!(media.kind, DxToolActionKind::CopyToClipboard);
		assert_eq!(media.confirmation, None);
	}
}
