package com.antihorizon.client;

import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

import java.util.List;

/**
 * Place and size the HUD. Drag a piece to move it (it snaps to the middle of the screen and to the edges of other
 * pieces), scroll over it to make it bigger or smaller, right-click it to put it back where it started. Positions are
 * saved as a share of the screen, so they fit any window size. Pieces with nothing to show yet (no potion effects, no
 * armour) show an example so they can still be placed.
 */
public final class HudEditor extends Screen {
	private static final int SNAP = 5;

	private final Screen parent;
	private Hud.Module dragging;
	private double grabX;
	private double grabY;
	private int guideX = -1;
	private int guideY = -1;
	private Hud.Module sized;
	private long sizedAt;

	HudEditor(Screen parent) {
		super(Component.literal("Edit HUD"));
		this.parent = parent;
	}

	Screen parent() {
		return parent;
	}

	private List<Hud.Module> pieces() {
		return Hud.ALL.stream().filter(Hud.Module::on).toList();
	}

	private boolean sample(Hud.Module m) {
		return !m.shows(minecraft);
	}

	private int[] rect(Hud.Module m) {
		return Hud.rect(m, minecraft, width, height, sample(m));
	}

	/** The piece under the mouse, topmost (last drawn) first. */
	private Hud.Module at(double x, double y) {
		List<Hud.Module> list = pieces();
		for (int i = list.size() - 1; i >= 0; i--) {
			int[] r = rect(list.get(i));
			if (x >= r[0] - 2 && x < r[0] + r[2] + 2 && y >= r[1] - 2 && y < r[1] + r[3] + 2) return list.get(i);
		}
		return null;
	}

	@Override
	public void renderBackground(GuiGraphics g, int mouseX, int mouseY, float delta) {
		// No blur: the point is to see the game behind the pieces.
		g.fill(0, 0, width, height, Ui.c(Ui.VOID, 0.35f));
		for (int x = 0; x < width; x += 20) Ui.vline(g, x, 0, height, Ui.c(Ui.BONE, 0.05f));
		for (int y = 0; y < height; y += 20) Ui.hline(g, 0, width, y, Ui.c(Ui.BONE, 0.05f));
		if (dragging != null) {
			if (guideX >= 0) Ui.vline(g, guideX, 0, height, Ui.c(Ui.accent(), 0.7f));
			if (guideY >= 0) Ui.hline(g, 0, width, guideY, Ui.c(Ui.accent(), 0.7f));
		}

		Ui.fade = 1f;
		Hud.Module hover = dragging != null ? dragging : at(mouseX, mouseY);
		for (Hud.Module m : pieces()) {
			int[] r = rect(m);
			boolean hot = m == hover;
			if (hot) Ui.round(g, r[0] - 2, r[1] - 2, r[0] + r[2] + 2, r[1] + r[3] + 2, 4, Ui.c(Ui.accent(), 0.1f));
			brackets(g, r, Ui.c(Ui.accent(), hot ? 0.95f : 0.4f));
			Hud.drawAt(m, g, minecraft, r, sample(m));
		}

		// Over the hovered piece: its name and size, so scrolling has something to read.
		if (hover != null) {
			int[] r = rect(hover);
			boolean justSized = hover == sized && System.currentTimeMillis() - sizedAt < 1200;
			String label = hover.name + "  ·  " + Math.round(Hud.scaleOf(hover) * 100) + "%" + (sample(hover) ? "  ·  example" : "");
			float lw = Txt.widthF(font, label, Txt.SMALL) + 12;
			float lx = Math.max(2, Math.min(width - lw - 2, r[0] + r[2] / 2f - lw / 2f));
			float ly = r[1] - 16 < 28 ? r[1] + r[3] + 4 : r[1] - 16;
			Ui.round(g, lx, ly, lx + lw, ly + 12, 6, Ui.c(justSized ? Ui.accent() : Ui.VOID, justSized ? 0.95f : 0.85f));
			Txt.middle(g, font, label, Txt.SMALL, lx + 6, ly, 12, justSized ? Ui.c(Ui.VOID, 1f) : Ui.c(Ui.BONE, 1f));
		}

		String hint = pieces().isEmpty()
				? "Nothing to place yet: switch on a HUD module in the menu first"
				: "Drag to move  ·  Scroll to resize  ·  Right-click to reset  ·  Esc to finish";
		int w = Txt.width(font, hint, Txt.UI) + 20;
		int x = (width - w) / 2;
		Ui.round(g, x, 8, x + w, 24, 8, Ui.c(Ui.VOID, 0.85f));
		Txt.middle(g, font, hint, Txt.UI, x + 10, 8, 16, Ui.c(Ui.BONE, 1f));
	}

	/** Four corner marks around a piece: shows its bounds without boxing it in. */
	private static void brackets(GuiGraphics g, int[] r, int color) {
		float x1 = r[0] - 2;
		float y1 = r[1] - 2;
		float x2 = r[0] + r[2] + 2;
		float y2 = r[1] + r[3] + 2;
		float a = Math.min(6, Math.min(r[2], r[3]) / 2f + 2);
		Ui.hline(g, x1, x1 + a, y1, color);
		Ui.vline(g, x1, y1, y1 + a, color);
		Ui.hline(g, x2 - a, x2, y1, color);
		Ui.vline(g, x2 - 1f / Ui.scale(), y1, y1 + a, color);
		Ui.hline(g, x1, x1 + a, y2 - 1f / Ui.scale(), color);
		Ui.vline(g, x1, y2 - a, y2, color);
		Ui.hline(g, x2 - a, x2, y2 - 1f / Ui.scale(), color);
		Ui.vline(g, x2 - 1f / Ui.scale(), y2 - a, y2, color);
	}

	@Override
	public boolean mouseClicked(MouseButtonEvent e, boolean doubleClick) {
		Hud.Module m = at(e.x(), e.y());
		if (m != null && e.button() == 0) {
			int[] r = rect(m);
			dragging = m;
			grabX = e.x() - r[0];
			grabY = e.y() - r[1];
			return true;
		}
		if (m != null && e.button() == 1) {
			// Back to where it started, at 100%.
			Config.positions.put(m.id, new float[] {m.x, m.y});
			Config.scales.remove(m.id);
			Config.save();
			sized = m;
			sizedAt = System.currentTimeMillis();
			return true;
		}
		return super.mouseClicked(e, doubleClick);
	}

	@Override
	public boolean mouseDragged(MouseButtonEvent e, double dx, double dy) {
		if (dragging == null) return super.mouseDragged(e, dx, dy);
		int[] r = rect(dragging);
		double nx = e.x() - grabX;
		double ny = e.y() - grabY;
		guideX = -1;
		guideY = -1;
		// The middle of the screen.
		double cx = (width - r[2]) / 2.0;
		double cy = (height - r[3]) / 2.0;
		if (Math.abs(nx - cx) < SNAP) {
			nx = cx;
			guideX = width / 2;
		}
		if (Math.abs(ny - cy) < SNAP) {
			ny = cy;
			guideY = height / 2;
		}
		// Line up with the other pieces: same left or right edge, same top, or stacked just under one.
		for (Hud.Module o : pieces()) {
			if (o == dragging) continue;
			int[] q = rect(o);
			if (guideX < 0 && Math.abs(nx - q[0]) < SNAP) {
				nx = q[0];
				guideX = q[0];
			} else if (guideX < 0 && Math.abs(nx + r[2] - (q[0] + q[2])) < SNAP) {
				nx = q[0] + q[2] - r[2];
				guideX = q[0] + q[2];
			}
			if (guideY < 0 && Math.abs(ny - q[1]) < SNAP) {
				ny = q[1];
				guideY = q[1];
			} else if (guideY < 0 && Math.abs(ny - (q[1] + q[3] + 2)) < SNAP) {
				ny = q[1] + q[3] + 2;
				guideY = q[1] + q[3] + 1;
			}
		}
		float[] pos = dragging.position();
		int spaceW = Math.max(1, width - 8 - r[2]);
		int spaceH = Math.max(1, height - 8 - r[3]);
		pos[0] = Config.clamp((float) ((nx - 4) / spaceW));
		pos[1] = Config.clamp((float) ((ny - 4) / spaceH));
		return true;
	}

	@Override
	public boolean mouseReleased(MouseButtonEvent e) {
		if (dragging != null) {
			dragging = null;
			guideX = -1;
			guideY = -1;
			Config.save();
			return true;
		}
		return super.mouseReleased(e);
	}

	@Override
	public boolean mouseScrolled(double x, double y, double scrollX, double scrollY) {
		Hud.Module m = dragging != null ? dragging : at(x, y);
		if (m == null || scrollY == 0) return super.mouseScrolled(x, y, scrollX, scrollY);
		// One step is the smallest change that stays on whole screen pixels (half a size at GUI scale 2).
		float step = Math.max(0.25f, 1f / Math.max(1, minecraft.getWindow().getGuiScale()));
		Config.setScale(m.id, Hud.scaleOf(m) + (scrollY > 0 ? step : -step));
		Config.save();
		sized = m;
		sizedAt = System.currentTimeMillis();
		return true;
	}

	@Override
	public boolean isPauseScreen() {
		return Config.pauseGame;
	}

	@Override
	public void onClose() {
		Config.save();
		minecraft.setScreen(parent instanceof ClientMenu ? parent : new ClientMenu(parent));
	}
}
