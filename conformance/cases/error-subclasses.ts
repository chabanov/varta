class UarpError extends Error {
  constructor(message: string, options?: { cause?: string }) {
    super(message, options);
    this.name = new.target.name;
  }
}

class APIError extends UarpError {
  readonly status: number;
  constructor(status: number) {
    super("HTTP " + String(status));
    this.status = status;
  }
}

class NotFoundError extends APIError {}

class Invalid extends TypeError {}

function request(status: number): string {
  if (status === 0) {
    throw new UarpError("Connection error", { cause: "socket closed" });
  }
  if (status === 404) {
    throw new NotFoundError(status);
  }
  throw new APIError(status);
}

export function main(): void {
  try {
    request(404);
  } catch (error) {
    console.log(error instanceof NotFoundError);
    console.log(error instanceof APIError && error.status === 404);
    if (error instanceof Error) {
      console.log(error.toString());
    }
  }
  try {
    request(0);
  } catch (error) {
    if (error instanceof UarpError) {
      console.log(error.name + ": " + error.message);
      console.log(error.cause === "socket closed");
      console.log(error instanceof APIError);
    }
  }
  try {
    try {
      request(500);
    } finally {
      console.log("finally");
    }
  } catch (error) {
    if (error instanceof APIError) {
      console.log(error.name + " " + String(error.status));
    }
  }
  const invalid = new Invalid("bad");
  console.log(invalid instanceof TypeError);
  console.log(invalid.toString());
}
