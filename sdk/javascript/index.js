const DEFAULT_HEADERS = {
  accept: "application/json",
};

export class MnemaraHttpError extends Error {
  constructor(message, { status, statusText, body }) {
    super(message);
    this.name = "MnemaraHttpError";
    this.status = status;
    this.statusText = statusText;
    this.body = body;
  }
}

export class MnemaraHttpClient {
  constructor({
    baseUrl,
    token,
    headers = {},
    fetchImpl = globalThis.fetch,
  } = {}) {
    if (!baseUrl) {
      throw new TypeError("baseUrl is required");
    }
    if (typeof fetchImpl !== "function") {
      throw new TypeError("A fetch-compatible implementation is required");
    }

    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.token = token ?? null;
    this.headers = { ...headers };
    this.fetchImpl = fetchImpl;
  }

  withToken(token) {
    return new MnemaraHttpClient({
      baseUrl: this.baseUrl,
      token,
      headers: this.headers,
      fetchImpl: this.fetchImpl,
    });
  }

  async health() {
    return this.#request("/healthz");
  }

  async ready() {
    return this.#request("/readyz");
  }

  async metrics() {
    const response = await this.#requestRaw("/metrics", {
      accept: "text/plain",
    });
    return response.text();
  }

  async upsert(request) {
    return this.#request("/memory/upsert", { method: "POST", body: request });
  }

  async batchUpsert(request) {
    return this.#request("/memory/batch-upsert", {
      method: "POST",
      body: request,
    });
  }

  async recall(query) {
    return this.#request("/memory/recall", { method: "POST", body: query });
  }

  async snapshot() {
    return this.#request("/admin/snapshot");
  }

  async stats(request = {}) {
    return this.#request("/admin/stats", { query: request });
  }

  async integrityCheck(request = {}) {
    return this.#request("/admin/integrity", { query: request });
  }

  async repair(request) {
    return this.#request("/admin/repair", { method: "POST", body: request });
  }

  async compact(request) {
    return this.#request("/admin/compact", { method: "POST", body: request });
  }

  async delete(request) {
    return this.#request("/admin/delete", { method: "POST", body: request });
  }

  async listTraces(request = {}) {
    return this.#request("/admin/traces", { query: request });
  }

  async getTrace(traceId) {
    return this.#request(`/admin/traces/${encodeURIComponent(traceId)}`);
  }

  async runtimeStatus() {
    return this.#request("/admin/runtime");
  }

  async export(request = {}) {
    return this.#request("/admin/export", { method: "POST", body: request });
  }

  async import(request) {
    return this.#request("/admin/import", { method: "POST", body: request });
  }

  async #request(path, options = {}) {
    const response = await this.#requestRaw(path, options);
    if (response.status === 204) {
      return null;
    }
    return response.json();
  }

  async #requestRaw(
    path,
    { method = "GET", body, query, accept, headers } = {},
  ) {
    const url = new URL(`${this.baseUrl}${path}`);
    if (query) {
      for (const [key, value] of Object.entries(query)) {
        if (value === undefined || value === null || value === "") {
          continue;
        }
        url.searchParams.set(key, String(value));
      }
    }

    const requestHeaders = {
      ...DEFAULT_HEADERS,
      ...this.headers,
      ...headers,
    };

    if (accept) {
      requestHeaders.accept = accept;
    }
    if (this.token) {
      requestHeaders.authorization = `Bearer ${this.token}`;
    }

    let requestBody;
    if (body !== undefined) {
      requestHeaders["content-type"] = "application/json";
      requestBody = JSON.stringify(body);
    }

    const response = await this.fetchImpl(url, {
      method,
      headers: requestHeaders,
      body: requestBody,
    });

    if (!response.ok) {
      let errorBody = null;
      const contentType = response.headers.get("content-type") || "";
      if (contentType.includes("application/json")) {
        errorBody = await response.json();
      } else {
        errorBody = await response.text();
      }
      throw new MnemaraHttpError(
        `Mnemara request failed with ${response.status} ${response.statusText}`,
        {
          status: response.status,
          statusText: response.statusText,
          body: errorBody,
        },
      );
    }

    return response;
  }
}

export const MemoryRecordKind = Object.freeze({
  Episodic: "Episodic",
  Summary: "Summary",
  Fact: "Fact",
  Preference: "Preference",
  Task: "Task",
  Artifact: "Artifact",
  Hypothesis: "Hypothesis",
});

export const MemoryQualityState = Object.freeze({
  Draft: "Draft",
  Active: "Active",
  Verified: "Verified",
  Archived: "Archived",
  Suppressed: "Suppressed",
  Deleted: "Deleted",
});

export const MemoryTrustLevel = Object.freeze({
  Untrusted: "Untrusted",
  Observed: "Observed",
  Derived: "Derived",
  Verified: "Verified",
  Pinned: "Pinned",
});

export const RecallScorerKind = Object.freeze({
  Profile: "Profile",
  Curated: "Curated",
});

export const RecallScoringProfile = Object.freeze({
  Balanced: "Balanced",
  LexicalFirst: "LexicalFirst",
  ImportanceFirst: "ImportanceFirst",
});

export const EmbeddingProviderKind = Object.freeze({
  Disabled: "Disabled",
  DeterministicLocal: "DeterministicLocal",
});
