// Deploy as Cloudflare Worker
// Set these secrets:
//   KEYGEN_ACCOUNT_ID  - your Keygen account UUID
//   KEYGEN_API_TOKEN   - your admin Bearer token
//   KEYGEN_POLICY_ID   - the policy UUID for Postilla licenses

export default {
  async fetch(req, env) {
    if (req.method !== "POST") {
      return new Response("Method not allowed", { status: 405 })
    }

    const { email } = await req.json()
    if (!email || !email.includes("@")) {
      return new Response(JSON.stringify({ error: "Invalid email address" }), {
        status: 400,
        headers: { "Content-Type": "application/json" },
      })
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
      // If user already exists, extract ID from error detail or search
      const existing = await fetch(`${BASE}/users?q=${encodeURIComponent(email)}`, { headers })
      if (existing.ok) {
        const users = await existing.json()
        userId = users.data?.[0]?.id
      }
      if (!userId) {
        return new Response(JSON.stringify({ error: "Failed to create user", detail: errBody }), {
          status: 500,
          headers: { "Content-Type": "application/json" },
        })
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
      return new Response(JSON.stringify({ error: "Failed to create license", detail: errBody }), {
        status: 500,
        headers: { "Content-Type": "application/json" },
      })
    }

    const lic = (await licRes.json()).data

    return new Response(JSON.stringify({ license_key: lic.attributes.key }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    })
  },
}
