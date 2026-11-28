// The Anti-Horizon cosmetics service: which cape, wings and head piece each player wears.
//
// It stores only a Minecraft player ID (UUID), the IDs of the cosmetics they picked and when. No names, emails,
// passwords, tokens, IP addresses or anything about where or how anyone plays.
//
//   GET  /v1/players?ids=<uuid>,<uuid>,...   what up to 100 players wear (the client mod asks about players near you)
//   POST /v1/login/start   {name}            a one-time server ID for Mojang's "joined" check
//   POST /v1/login/finish  {name, serverId}  after the launcher joined with it: Mojang confirms the account is yours
//   GET  /v1/me / PUT /v1/me {cape, wings, head}   your own cosmetics (with the token from login/finish)
//
// Signing in works the way Minecraft servers check players: the launcher tells Mojang it is joining with the server
// ID, and this service asks Mojang whether that player did. It never sees a password or a Microsoft/Minecraft token.
import catalog from "../../catalog.json";

const KNOWN = {
  cape: new Set(catalog.cape.map((c) => c.id)),
  // Capes only for now: wings and head pieces aren't released, so they can't be worn yet.
  wings: new Set(),
  head: new Set(),
};
const SLOTS = ["cape", "wings", "head"];
const UUID = /^[0-9a-f]{32}$/;
const NAME = /^[A-Za-z0-9_]{1,16}$/;
const TOKEN_DAYS = 30;
const enc = new TextEncoder();

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const route = `${request.method} ${url.pathname}`;
    try {
      switch (route) {
        case "GET /v1/players":
          return await players(url, env);
        case "POST /v1/login/start":
          return await loginStart(request, env);
        case "POST /v1/login/finish":
          return await loginFinish(request, env);
        case "GET /v1/me":
          return await getMe(request, env);
        case "PUT /v1/me":
          return await putMe(request, env);
        case "GET /v1/catalog":
          return json(catalog, 200, { "Cache-Control": "public, max-age=300" });
        case "GET /":
          return new Response(
            "Anti-Horizon cosmetics service. What it stores and why: https://anti-horizon.com/privacy/\n",
            { headers: { "Content-Type": "text/plain; charset=utf-8" } },
          );
        default:
          return json({ error: "Not found" }, 404);
      }
    } catch (err) {
      if (err instanceof Bad) return json({ error: err.message }, err.status);
      console.error(err);
      return json({ error: "Something went wrong" }, 500);
    }
  },
};

class Bad extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

function json(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", "X-Content-Type-Options": "nosniff", ...headers },
  });
}

async function body(request) {
  if ((request.headers.get("Content-Length") | 0) > 4096) throw new Bad(413, "Too big");
  try {
    return await request.json();
  } catch {
    throw new Bad(400, "Send JSON");
  }
}

// ---------------------------------------------------------------- looks

async function players(url, env) {
  const ids = [...new Set((url.searchParams.get("ids") || "").toLowerCase().split(","))].filter((id) => UUID.test(id));
  if (ids.length === 0) return json({}, 200, { "Cache-Control": "public, max-age=60" });
  if (ids.length > 100) throw new Bad(400, "At most 100 players at a time");
  const { results } = await env.DB.prepare(
    `SELECT uuid, cape, wings, head FROM looks WHERE uuid IN (${ids.map(() => "?").join(",")})`,
  ).bind(...ids).all();
  const out = {};
  for (const r of results) out[r.uuid] = clean(r);
  return json(out, 200, { "Cache-Control": "public, max-age=60" });
}

function clean(r) {
  const look = {};
  for (const s of SLOTS) if (r[s]) look[s] = r[s];
  return look;
}

async function getMe(request, env) {
  const uuid = await signedIn(request, env);
  const row = await env.DB.prepare("SELECT cape, wings, head FROM looks WHERE uuid = ?").bind(uuid).first();
  return json({ uuid, ...(row ? clean(row) : {}) });
}

async function putMe(request, env) {
  const uuid = await signedIn(request, env);
  const b = await body(request);
  const look = {};
  for (const s of SLOTS) {
    const v = b[s] ?? null;
    if (v !== null && !(typeof v === "string" && KNOWN[s].has(v))) throw new Bad(400, `Unknown ${s}`);
    look[s] = v;
  }
  if (SLOTS.every((s) => look[s] === null)) {
    await env.DB.prepare("DELETE FROM looks WHERE uuid = ?").bind(uuid).run();
  } else {
    await env.DB.prepare(
      `INSERT INTO looks (uuid, cape, wings, head, updated) VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(uuid) DO UPDATE SET cape = excluded.cape, wings = excluded.wings, head = excluded.head, updated = excluded.updated`,
    ).bind(uuid, look.cape, look.wings, look.head, Date.now()).run();
  }
  return json({ uuid, ...clean(look) });
}

// ---------------------------------------------------------------- signing in

async function hmac(env, text) {
  if (!env.SECRET || env.SECRET.length < 32) throw new Error("SECRET isn't set");
  const key = await crypto.subtle.importKey("raw", enc.encode(env.SECRET), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const sig = await crypto.subtle.sign("HMAC", key, enc.encode(text));
  return [...new Uint8Array(sig)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function same(a, b) {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/** The server ID for this player and minute: made here, so a Minecraft server elsewhere can't use a join it saw. */
async function serverId(env, name, minute) {
  return (await hmac(env, `join|${name.toLowerCase()}|${minute}`)).slice(0, 40);
}

async function loginStart(request, env) {
  const { name } = await body(request);
  if (typeof name !== "string" || !NAME.test(name)) throw new Bad(400, "Not a Minecraft name");
  return json({ serverId: await serverId(env, name, Math.floor(Date.now() / 60000)) });
}

async function loginFinish(request, env) {
  const { name, serverId: sid } = await body(request);
  if (typeof name !== "string" || !NAME.test(name) || typeof sid !== "string") throw new Bad(400, "Name and server ID, please");
  const minute = Math.floor(Date.now() / 60000);
  const ok = same(sid, await serverId(env, name, minute)) || same(sid, await serverId(env, name, minute - 1));
  if (!ok) throw new Bad(400, "That server ID is old or not from here; start again");
  const check = new URL(env.MOJANG_HAS_JOINED || "https://sessionserver.mojang.com/session/minecraft/hasJoined");
  check.searchParams.set("username", name);
  check.searchParams.set("serverId", sid);
  const res = await fetch(check, { headers: { "User-Agent": "AntiHorizonCosmetics" } });
  if (res.status !== 200) throw new Bad(403, "Mojang didn't confirm the sign-in");
  const profile = await res.json();
  const uuid = String(profile.id || "").toLowerCase();
  if (!UUID.test(uuid)) throw new Bad(502, "Mojang's answer had no player ID");
  const exp = Math.floor(Date.now() / 1000) + TOKEN_DAYS * 86400;
  const token = `${uuid}.${exp}.${await hmac(env, `token|${uuid}|${exp}`)}`;
  return json({ token, uuid, name: profile.name, expires: exp });
}

async function signedIn(request, env) {
  const auth = request.headers.get("Authorization") || "";
  const [uuid, exp, sig] = auth.replace(/^Bearer\s+/i, "").split(".");
  if (!UUID.test(uuid || "") || !/^\d+$/.test(exp || "") || Number(exp) < Date.now() / 1000) throw new Bad(401, "Sign in again");
  if (!same(sig, await hmac(env, `token|${uuid}|${exp}`))) throw new Bad(401, "Sign in again");
  return uuid;
}
