// Deploy as Cloudflare Worker
// Set these secrets:
//   KEYGEN_ACCOUNT_ID  - your Keygen account UUID
//   KEYGEN_API_TOKEN   - your admin Bearer token
//   KEYGEN_POLICY_ID   - the policy UUID for Postilla licenses
//   GITHUB_REPO        - repo for updates (es. francescozanti/postilla)

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "POST, GET, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
}

export default {
  async fetch(req, env) {
    if (req.method === "OPTIONS") {
      return new Response(null, { headers: CORS })
    }

    const url = new URL(req.url)

    if (url.pathname === "/update") {
      return handleUpdate(req, env)
    }

    return handleLicense(req, env)
  },
}

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...CORS, "Content-Type": "application/json" },
  })
}

async function handleLicense(req, env) {
  if (req.method !== "POST") {
    return json({ error: "Method not allowed" }, 405)
  }

  const { email, deviceId } = await req.json()
  if (!email || !email.includes("@")) {
    return json({ error: "Invalid email address" }, 400)
  }

  const { KEYGEN_ACCOUNT_ID, KEYGEN_API_TOKEN, KEYGEN_POLICY_ID } = env
  const BASE = `https://api.keygen.sh/v1/accounts/${KEYGEN_ACCOUNT_ID}`
  const headers = {
    Authorization: `Bearer ${KEYGEN_API_TOKEN}`,
    "Content-Type": "application/vnd.api+json",
    Accept: "application/vnd.api+json",
  }

  // 1. Create or find existing user
  const userRes = await fetch(`${BASE}/users`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      data: {
        type: "users",
        attributes: { email },
      },
    }),
  })

  let userId
  if (userRes.ok) {
    userId = (await userRes.json()).data.id
  } else {
    const errBody = await userRes.json()
    const existing = await fetch(`${BASE}/users?q=${encodeURIComponent(email)}`, { headers })
    if (existing.ok) {
      const users = await existing.json()
      userId = users.data?.[0]?.id
    }
    if (!userId) {
      return json({ error: "Failed to create user", detail: errBody }, 500)
    }
  }

  // 2. Create a license for this user
  const licRes = await fetch(`${BASE}/licenses`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      data: {
        type: "licenses",
        attributes: {
          metadata: { email, source: "claim" },
        },
        relationships: {
          policy: {
            data: { type: "policies", id: KEYGEN_POLICY_ID },
          },
          owner: {
            data: { type: "users", id: userId },
          },
        },
      },
    }),
  })

  if (!licRes.ok) {
    const errBody = await licRes.json()
    return json({ error: "Failed to create license", detail: errBody }, 500)
  }

  const lic = (await licRes.json()).data
  const licenseKey = lic.attributes.key

  // 3. Create a machine (device binding) if deviceId provided
  if (deviceId) {
    const machRes = await fetch(`${BASE}/machines`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${KEYGEN_API_TOKEN}`,
        "Content-Type": "application/vnd.api+json",
        Accept: "application/vnd.api+json",
      },
      body: JSON.stringify({
        data: {
          type: "machines",
          attributes: {
            fingerprint: deviceId,
            name: `Device ${deviceId.slice(0, 8)}`,
          },
          relationships: {
            license: {
              data: { type: "licenses", id: lic.id },
            },
          },
        },
      }),
    })

    if (!machRes.ok) {
      const machErr = await machRes.text()
      console.error("Machine creation failed:", machErr)
    }
  }

  return json({ license_key: licenseKey })
}

async function handleUpdate(req, env) {
  const url = new URL(req.url)
  const version = url.searchParams.get("version") ?? ""
  const target = url.searchParams.get("target") ?? ""
  const repo = env.GITHUB_REPO || "francescozanti/postilla"

  const res = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
    headers: {
      Accept: "application/vnd.github.v3+json",
      "User-Agent": "postilla-updater",
    },
  })

  if (!res.ok) {
    return new Response(JSON.stringify({ error: "Failed to fetch release" }), {
      status: 502,
      headers: { "Content-Type": "application/json" },
    })
  }

  const release = await res.json()
  const tag = release.tag_name
  const ver = tag.startsWith("v") ? tag.slice(1) : tag

  const extMap = {
    "linux-x86_64": ".deb",
    "windows-x86_64": ".msi",
    "darwin-x86_64": ".dmg",
    "darwin-aarch64": ".dmg",
  }
  const ext = extMap[target] || ".AppImage"
  const assetName = `postilla_${ver}_${target}${ext}`

  const asset = release.assets.find((a) => a.name === assetName)
  const sig = release.assets.find((a) => a.name === `${assetName}.sig`)

  if (!asset) {
    return new Response(
      JSON.stringify({ error: `Asset not found for ${ver} / ${target}` }),
      { status: 404, headers: { "Content-Type": "application/json" } }
    )
  }

  let signature = ""
  if (sig) {
    const sigRes = await fetch(sig.browser_download_url)
    signature = (await sigRes.text()).trim()
  }

  const payload = {
    version: ver,
    notes: release.body || "",
    pub_date: release.published_at,
    platforms: {
      [target]: {
        url: asset.browser_download_url,
        signature,
      },
    },
  }

  return new Response(JSON.stringify(payload), {
    headers: {
      "Content-Type": "application/json",
      "Access-Control-Allow-Origin": "*",
    },
  })
}
