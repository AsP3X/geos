/** API error envelope from the Geos backend. */
export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    fields?: Record<string, string>;
  };
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly fields?: Record<string, string>;

  constructor(status: number, code: string, message: string, fields?: Record<string, string>) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.fields = fields;
  }
}

export async function parseApiError(response: Response): Promise<ApiError> {
  const fallback = new ApiError(
    response.status,
    "unknown",
    response.statusText || "Request failed",
  );

  try {
    const body = (await response.json()) as ApiErrorBody;
    if (body.error?.message) {
      return new ApiError(
        response.status,
        body.error.code ?? "unknown",
        body.error.message,
        body.error.fields,
      );
    }
  } catch {
    // Non-JSON error body — use fallback.
  }

  return fallback;
}
