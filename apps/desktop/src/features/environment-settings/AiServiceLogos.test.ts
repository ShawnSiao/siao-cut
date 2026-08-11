import { describe, expect, it } from "vitest";
import anthropicLogo from "../../assets/ai-service-logos/anthropic.svg?raw";
import chatGlmLogo from "../../assets/ai-service-logos/chatglm.svg?raw";
import codexLogo from "../../assets/ai-service-logos/codex.svg?raw";
import deepSeekLogo from "../../assets/ai-service-logos/deepseek.svg?raw";
import geminiLogo from "../../assets/ai-service-logos/gemini.svg?raw";
import kimiLogo from "../../assets/ai-service-logos/kimi.svg?raw";
import openAiLogo from "../../assets/ai-service-logos/openai.svg?raw";

const logoColors = [
  ["OpenAI", openAiLogo, ["#000000"]],
  ["Anthropic", anthropicLogo, ["#141413"]],
  ["Gemini", geminiLogo, ["#3186FF", "#08B962", "#F94543", "#FABC12"]],
  ["DeepSeek", deepSeekLogo, ["#4D6BFE"]],
  ["Kimi", kimiLogo, ["#1783FF", "#000000"]],
  ["GLM", chatGlmLogo, ["#504AF4", "#3485FF"]],
  ["Codex", codexLogo, ["#B1A7FF", "#7A9DFF", "#3941FF"]],
] as const;

describe("AI service brand logos", () => {
  it.each(logoColors)("keeps %s brand colors in the SVG asset", (_name, svg, colors) => {
    expect(svg).not.toContain("currentColor");
    for (const color of colors) expect(svg).toContain(color);
  });
});
