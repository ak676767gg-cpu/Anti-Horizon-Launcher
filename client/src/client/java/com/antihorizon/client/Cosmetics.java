package com.antihorizon.client;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.minecraft.core.ClientAsset;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.player.PlayerSkin;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Anti-Horizon cosmetics: a cape, wings and something on the head, picked in the launcher.
 *
 * Who wears what comes from the cosmetics service, which only ever learns the IDs of players near you (never names,
 * chat or where you are). Your own choice is also in ~/.anti-horizon/cosmetics-me.json, which the launcher writes when
 * you change it, so it shows in game straight away. CosmeticsLayer draws capes (with the black hole's disk turning),
 * wings and head pieces; SkinMixin puts the cape's art on elytra and keeps Minecraft from drawing a second cape.
 */
public final class Cosmetics {
	/** One player's cosmetics; null for an empty slot. */
	/**
	 * Where a cape's black hole is and how its disk lies, so the client can turn the disk around it: the centre and the
	 * still middle (hole) and where the motion has faded out (outer), in widths of the cape's face; the disk's tilt
	 * (degrees, clockwise), how flat it looks (1 = face on), which way it turns (1 or -1), and how much of the disk is
	 * the turning copy (1 = all of it).
	 */
	public record Flow(float x, float y, float hole, float outer, float tilt, float ratio, float spin, float mix) {
	}

	/** A live cape's clip: its AHV1 file on anti-horizon.com, how big it is and its SHA-256. */
	public record Clip(String file, long size, String sha256) {
	}

	public record Look(String cape, String wings, String head) {
		static final Look NONE = new Look(null, null, null);

		boolean empty() {
			return cape == null && wings == null && head == null;
		}
	}

	private record Entry(Look look, long fetched) {
	}

	/** Cosmetics aren't released yet: until they are, the client neither asks the service nor draws any. */
	public static final boolean RELEASED = true;
	/** Capes only for now: wings and head pieces aren't released yet, so they're never drawn. */
	static final boolean CAPES_ONLY = true;
	private static final String SERVICE = System.getProperty("antihorizon.cosmetics", "https://cosmetics.anti-horizon.com");
	private static final Path MINE = Path.of(System.getProperty("user.home"), ".anti-horizon", "cosmetics-me.json");
	private static final long FRESH_MS = 5 * 60_000;
	private static final long RETRY_MS = 2 * 60_000;
	private static final int BATCH = 100;

	private static final Map<UUID, Entry> LOOKS = new ConcurrentHashMap<>();
	private static final Set<UUID> ASKING = ConcurrentHashMap.newKeySet();
	private static final Set<String> CAPES = new HashSet<>();
	private static final Set<String> WINGS = new HashSet<>();
	private static final Set<String> HEAD = new HashSet<>();
	private static final Map<String, Flow> FLOWS = new HashMap<>();
	private static final Map<String, Identifier> FACES = new HashMap<>();
	private static final Map<String, Clip> CLIPS = new HashMap<>();
	private static final ExecutorService NET = Executors.newSingleThreadExecutor(r -> {
		Thread t = new Thread(r, "Anti-Horizon cosmetics");
		t.setDaemon(true);
		t.setPriority(Thread.MIN_PRIORITY);
		return t;
	});
	private static HttpClient http;
	private static volatile long failedUntil;
	private static long mineModified = -1;
	private static int ticks;

	// The cape as a PlayerSkin, per player, so getSkin (called every frame) doesn't make a new one each time.
	private static final Map<UUID, Object[]> SKINS = new ConcurrentHashMap<>();

	private Cosmetics() {
	}

	/** Reads which cosmetics this copy of the client knows: a newer one someone wears is skipped, not drawn wrong. */
	static void load() {
		try (InputStream in = Cosmetics.class.getResourceAsStream("/assets/antihorizon/textures/cosmetics/catalog.json")) {
			if (in == null) return;
			JsonObject c = JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8)).getAsJsonObject();
			for (JsonElement e : c.getAsJsonArray("cape")) {
				JsonObject o = e.getAsJsonObject();
				String id = o.get("id").getAsString();
				CAPES.add(id);
				JsonObject clip = o.getAsJsonObject("clip");
				if (clip != null) CLIPS.put(id, new Clip(clip.get("file").getAsString(), clip.get("size").getAsLong(), clip.get("sha256").getAsString()));
				JsonObject f = o.getAsJsonObject("flow");
				if (f != null) {
					FLOWS.put(id, new Flow(f.get("x").getAsFloat(), f.get("y").getAsFloat(), f.get("hole").getAsFloat(),
							f.get("outer").getAsFloat(), f.get("tilt").getAsFloat(), f.get("ratio").getAsFloat(), f.get("spin").getAsFloat(),
							f.has("mix") ? f.get("mix").getAsFloat() : 1f));
				}
			}
			for (JsonElement e : c.getAsJsonArray("wings")) WINGS.add(e.getAsJsonObject().get("id").getAsString());
			for (JsonElement e : c.getAsJsonArray("head")) HEAD.add(e.getAsJsonObject().get("id").getAsString());
		} catch (Exception e) {
			AntiHorizonClient.LOG.warn("Couldn't read the cosmetics list", e);
		}
	}

	public static Identifier texture(String kind, String id) {
		return Identifier.fromNamespaceAndPath("antihorizon", "textures/cosmetics/" + kind + "/" + id + ".png");
	}

	/** The clip of a live cape, or null for a still one. */
	public static Clip clip(String cape) {
		return CLIPS.get(cape);
	}

	public static Flow flow(String cape) {
		return FLOWS.get(cape);
	}

	/**
	 * The outside of a cape: a sharp picture, smoothly filtered (Minecraft's own textures are blocky up close), or a
	 * smaller one for capes far away so they don't shimmer. Loaded the first time it's needed; null if it can't be.
	 */
	public static Identifier face(String cape, boolean far) {
		String key = "cape-" + cape + (far ? "-far" : "");
		if (FACES.containsKey(key)) return FACES.get(key);
		Identifier id = null;
		try (InputStream in = Cosmetics.class.getResourceAsStream("/assets/antihorizon/textures/cosmetics/cape/" + cape + (far ? "-face-far" : "-face") + ".png")) {
			if (in != null) {
				MenuVideo.Tex t = MenuVideo.picture(key, in.readAllBytes());
				if (t != null) id = t.id();
			}
		} catch (Exception e) {
			AntiHorizonClient.LOG.warn("Couldn't load the {} cape", cape, e);
		}
		FACES.put(key, id);
		return id;
	}

	/** What a player wears, or null when nothing (or cosmetics are switched off). */
	public static Look of(UUID id) {
		if (!Config.cosmetics) return null;
		Entry e = LOOKS.get(id);
		return e == null || e.look.empty() ? null : e.look;
	}

	/** What the player with this entity ID wears: how the render layer finds out whose body it is drawing. */
	public static Look ofEntity(int entityId) {
		Minecraft mc = Minecraft.getInstance();
		if (mc.level == null || !Config.cosmetics) return null;
		Entity e = mc.level.getEntity(entityId);
		return e instanceof Player p ? of(p.getUUID()) : null;
	}

	/**
	 * The player's skin as Minecraft should draw it when they wear an Anti-Horizon cape: no cape of its own (the layer
	 * draws ours, moving), and the cape's art on their elytra.
	 */
	public static PlayerSkin skin(AbstractClientPlayer player, PlayerSkin skin) {
		if (!RELEASED) return skin;
		Look look = of(player.getUUID());
		if (look == null || look.cape == null) return skin;
		Object[] cached = SKINS.get(player.getUUID());
		if (cached != null && cached[0] == skin && look.cape.equals(cached[1])) return (PlayerSkin) cached[2];
		Identifier tex = texture("cape", look.cape);
		ClientAsset.ResourceTexture elytra = new ClientAsset.ResourceTexture(tex, tex);
		PlayerSkin withCape = new PlayerSkin(skin.body(), null, elytra, skin.model(), skin.secure());
		SKINS.put(player.getUUID(), new Object[] {skin, look.cape, withCape});
		return withCape;
	}

	/** Every second: picks up the launcher's file for your own look, and asks about players it hasn't heard of yet. */
	static void tick(Minecraft mc) {
		if (++ticks % 20 != 0 || !Config.cosmetics) return;
		readMine(mc);
		if (mc.level == null || ticks % 40 != 0) return;
		long now = System.currentTimeMillis();
		if (now < failedUntil) return;
		List<UUID> ask = new ArrayList<>();
		for (Player p : mc.level.players()) {
			UUID id = p.getUUID();
			Entry e = LOOKS.get(id);
			if ((e == null || now - e.fetched > FRESH_MS) && !ASKING.contains(id)) ask.add(id);
			if (ask.size() == BATCH) break;
		}
		if (ask.isEmpty()) return;
		ASKING.addAll(ask);
		NET.execute(() -> fetch(ask));
	}

	private static void fetch(List<UUID> ids) {
		try {
			if (http == null) http = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(8)).build();
			StringBuilder q = new StringBuilder();
			for (UUID id : ids) q.append(q.isEmpty() ? "" : ",").append(id.toString().replace("-", ""));
			HttpRequest req = HttpRequest.newBuilder(URI.create(SERVICE + "/v1/players?ids=" + q))
					.timeout(Duration.ofSeconds(10))
					.header("User-Agent", "AntiHorizonClient")
					.GET().build();
			HttpResponse<String> res = http.send(req, HttpResponse.BodyHandlers.ofString());
			if (res.statusCode() != 200) throw new IllegalStateException("HTTP " + res.statusCode());
			JsonObject all = JsonParser.parseString(res.body()).getAsJsonObject();
			long now = System.currentTimeMillis();
			for (UUID id : ids) {
				JsonElement e = all.get(id.toString().replace("-", ""));
				Entry old = LOOKS.get(id);
				// Your own look from the launcher's file is newer than anything the service had a moment ago.
				if (old != null && old.fetched > now) continue;
				LOOKS.put(id, new Entry(e != null && e.isJsonObject() ? look(e.getAsJsonObject()) : Look.NONE, now));
			}
		} catch (Exception e) {
			failedUntil = System.currentTimeMillis() + RETRY_MS;
		} finally {
			ids.forEach(ASKING::remove);
		}
	}

	/** The launcher writes {"uuid": "...", "cape": "...", ...} here when you change your cosmetics. */
	private static void readMine(Minecraft mc) {
		try {
			long modified = Files.exists(MINE) ? Files.getLastModifiedTime(MINE).toMillis() : 0;
			if (modified == mineModified) return;
			mineModified = modified;
			if (modified == 0) return;
			JsonObject o = JsonParser.parseString(Files.readString(MINE)).getAsJsonObject();
			UUID id = parse(o.get("uuid").getAsString());
			// Marked as fetched a little in the future, so a slower answer from the service can't undo it.
			LOOKS.put(id, new Entry(look(o), System.currentTimeMillis() + 30_000));
			SKINS.remove(id);
		} catch (Exception ignored) {
			// half-written or hand-edited: the next change fixes it
		}
	}

	private static Look look(JsonObject o) {
		return new Look(pick(o, "cape", CAPES), CAPES_ONLY ? null : pick(o, "wings", WINGS), CAPES_ONLY ? null : pick(o, "head", HEAD));
	}

	private static String pick(JsonObject o, String key, Set<String> known) {
		JsonElement e = o.get(key);
		if (e == null || !e.isJsonPrimitive()) return null;
		String id = e.getAsString();
		return known.contains(id) ? id : null;
	}

	private static UUID parse(String s) {
		String h = s.replace("-", "");
		return new UUID(Long.parseUnsignedLong(h.substring(0, 16), 16), Long.parseUnsignedLong(h.substring(16), 16));
	}
}
