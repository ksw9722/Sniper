function quoteJsonString(token, quote) {
  const originalQuote = token[0];
  if (originalQuote === quote) return token;
  const content = token.slice(1, -1).replace(/\\[\s\S]|['"]/g, (part) => {
    if (part === "\\" + originalQuote) return originalQuote;
    if (part === quote) return "\\" + quote;
    return part;
  });
  return quote + content + quote;
}

function normalizeDictLikeString(dictString) {
  // Dict mode accepts JSON-like syntax with either string delimiter, not Python expressions.
  return dictString.replace(/"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|["']/g, (token) => {
    if (token.length === 1) throw new SyntaxError("Unterminated string");
    return quoteJsonString(token, '"');
  });
}

function formatJsonText(jsonString, quote = '"') {
  // Validate without serializing the parsed value: numbers, duplicate keys and
  // escape spelling are evidence that a beautifier must not silently rewrite.
  JSON.parse(jsonString);
  const tokens = jsonString.match(/"(?:\\[\s\S]|[^"\\])*"|[{}\[\],:]|[^\s{}\[\],:]+/g);
  const output = [];
  let depth = 0;
  const newline = () => "\n" + "  ".repeat(depth);
  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index];
    if (token === "{" || token === "[") {
      output.push(token);
      depth++;
      if (tokens[index + 1] !== "}" && tokens[index + 1] !== "]") output.push(newline());
    } else if (token === "}" || token === "]") {
      depth--;
      if (tokens[index - 1] !== "{" && tokens[index - 1] !== "[") output.push(newline());
      output.push(token);
    } else if (token === ",") {
      output.push(",", newline());
    } else if (token === ":") {
      output.push(": ");
    } else {
      output.push(token[0] === '"' ? quoteJsonString(token, quote) : token);
    }
  }
  return output.join("");
}

const customJson = {
  jsonBeautifier: (jsonString) => {
    try {
      return formatJsonText(jsonString);
    } catch (error) {
      return "";
    }
  },

  dictBeautifier: (dictString) => {
    try {
      return formatJsonText(normalizeDictLikeString(dictString), "'");
    } catch (error) {
      return "";
    }
  }
};
