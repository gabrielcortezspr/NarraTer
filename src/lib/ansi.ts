const ANSI_RE =
  /[\x1B\x9B][[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><~]|[\x1B\x9B]\][^\x07\x1B]*(?:\x07|\x1B\\)/g;

export function stripAnsi(str: string): string {
  return str.replace(ANSI_RE, "").replace(/\r\n/g, "\n").replace(/\r/g, "\n").replace(/\x00/g, "");
}

/** Keep only printable lines (strip blank lines from shell prompts, etc.) */
export function cleanLines(str: string): string {
  return str
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .join("\n");
}
