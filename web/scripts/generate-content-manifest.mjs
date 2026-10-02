import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { format } from "oxfmt";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(scriptDir, "..");
const repoDir = path.resolve(webDir, "..");
const contentDir = path.join(repoDir, "crates", "ti4-content", "content");
const outputPath = path.join(webDir, "src", "protocol", "generatedContentManifest.ts");
const generatorVersion = 1;

function requiredString(record, field, category) {
  const value = record[field];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${category} record is missing required string field ${field}`);
  }
  return value;
}

function optionalString(record, field) {
  const value = record[field];
  return typeof value === "string" ? value : "";
}

function addEntry(catalog, id, meta, category) {
  const entry = { id: meta.id ?? id, ...meta };
  const existing = catalog[id];
  const existingPresentation = { ...existing };
  const entryPresentation = { ...entry };
  delete existingPresentation.id;
  delete entryPresentation.id;
  if (existing && JSON.stringify(existingPresentation) !== JSON.stringify(entryPresentation)) {
    throw new Error(`${category} assigns conflicting presentation metadata to ${id}`);
  }
  catalog[id] ??= entry;
}

async function readJson(filename) {
  return JSON.parse(await readFile(path.join(contentDir, filename), "utf8"));
}

function sortedCatalog(catalog) {
  return Object.fromEntries(
    Object.entries(catalog).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  );
}

async function generate() {
  const [
    manifest,
    strategyCards,
    secretObjectives,
    publicObjectives,
    actionCards,
    technologies,
    explores,
    planets,
    attachments,
  ] = await Promise.all([
    readJson("manifest.json"),
    readJson("strategy_cards.json"),
    readJson("secret_objectives.json"),
    readJson("public_objectives.json"),
    readJson("action_cards.json"),
    readJson("technologies.json"),
    readJson("explores.json"),
    readJson("planets.json"),
    readJson("attachments.json"),
  ]);

  const strategyCatalog = {};
  for (const card of strategyCards) {
    const id = requiredString(card, "id", "strategy_cards");
    addEntry(
      strategyCatalog,
      id,
      {
        name: requiredString(card, "name", "strategy_cards"),
        initiative: card.initiative,
        primaryText: Array.isArray(card.primaryTexts) ? card.primaryTexts.join("\n") : "",
        secondaryText: Array.isArray(card.secondaryTexts) ? card.secondaryTexts.join("\n") : "",
      },
      "strategy_cards",
    );
  }

  const objectiveCatalog = (records, category) => {
    const catalog = {};
    for (const record of records) {
      const id = requiredString(record, "alias", category);
      addEntry(
        catalog,
        id,
        {
          id,
          name: requiredString(record, "name", category),
          phase: optionalString(record, "phase") || "Status",
          points: record.points ?? 1,
          description: optionalString(record, "text"),
        },
        category,
      );
    }
    return catalog;
  };

  const cardCatalog = (records, category) => {
    const catalog = {};
    for (const record of records) {
      const id = requiredString(record, "alias", category);
      const meta = {
        id,
        name: requiredString(record, "name", category),
        phase: optionalString(record, "phase") || undefined,
        description: optionalString(record, "text"),
      };
      addEntry(catalog, id, meta, category);

      // The engine may expose an action card by its corpus-declared automation ID.
      if (typeof record.automationID === "string" && record.automationID.length > 0) {
        addEntry(catalog, record.automationID, { ...meta, id: record.automationID }, category);
      }
    }
    return catalog;
  };

  const technologyCatalog = (records) => {
    const catalog = {};
    for (const record of records) {
      const id = requiredString(record, "alias", "technologies");
      const meta = {
        id,
        name: requiredString(record, "name", "technologies"),
        types: Array.isArray(record.types) ? record.types : [],
        requirements: optionalString(record, "requirements") || undefined,
        faction: optionalString(record, "faction") || undefined,
        source: optionalString(record, "source") || undefined,
        baseUpgrade: optionalString(record, "baseUpgrade") || undefined,
        description: optionalString(record, "text"),
      };
      addEntry(catalog, id, meta, "technologies");
    }
    return catalog;
  };

  const exploreCatalog = {};
  for (const card of explores) {
    const id = requiredString(card, "id", "explores");
    addEntry(
      exploreCatalog,
      id,
      {
        id,
        name: requiredString(card, "name", "explores"),
        type: requiredString(card, "type", "explores"),
        resolution: requiredString(card, "resolution", "explores"),
        description: optionalString(card, "text"),
        flavorText:
          optionalString(card, "flavorText") || optionalString(card, "flavourText") || undefined,
      },
      "explores",
    );
  }

  const planetCatalog = {};
  for (const planet of planets) {
    const id = requiredString(planet, "id", "planets");
    addEntry(
      planetCatalog,
      id,
      {
        id,
        name: requiredString(planet, "name", "planets"),
        resources: planet.resources ?? 0,
        influence: planet.influence ?? 0,
        techSpecialties: Array.isArray(planet.techSpecialties) ? planet.techSpecialties : [],
      },
      "planets",
    );
  }

  const attachmentCatalog = {};
  for (const attachment of attachments) {
    const id = requiredString(attachment, "id", "attachments");
    addEntry(
      attachmentCatalog,
      id,
      {
        id,
        name: optionalString(attachment, "name") || id,
        resourcesModifier: attachment.resourcesModifier ?? 0,
        influenceModifier: attachment.influenceModifier ?? 0,
      },
      "attachments",
    );
  }

  const catalogs = {
    strategyCards: sortedCatalog(strategyCatalog),
    secretObjectives: sortedCatalog(objectiveCatalog(secretObjectives, "secret_objectives")),
    publicObjectives: sortedCatalog(objectiveCatalog(publicObjectives, "public_objectives")),
    actionCards: sortedCatalog(cardCatalog(actionCards, "action_cards")),
    technologies: sortedCatalog(technologyCatalog(technologies)),
    explorationCards: sortedCatalog(exploreCatalog),
    planets: sortedCatalog(planetCatalog),
    attachments: sortedCatalog(attachmentCatalog),
  };
  const source = JSON.stringify(catalogs);
  const sourceDigest = createHash("sha256").update(source).digest("hex");
  const output = `// This file is generated by web/scripts/generate-content-manifest.mjs. Do not edit.\n\nexport const CONTENT_PRESENTATION_PROVENANCE = ${JSON.stringify(
    {
      generatorVersion,
      corpusSchemaVersion: manifest.schema_version,
      corpusUpstreamCommit: manifest.upstream.commit,
      presentationSha256: sourceDigest,
      recordCounts: {
        strategyCards: strategyCards.length,
        secretObjectives: secretObjectives.length,
        publicObjectives: publicObjectives.length,
        actionCards: actionCards.length,
        technologies: technologies.length,
        explorationCards: explores.length,
        planets: planets.length,
        attachments: attachments.length,
      },
    },
    null,
    2,
  )} as const;\n\nexport const GENERATED_CONTENT_CATALOG = ${JSON.stringify(catalogs, null, 2)} as const;\n`;
  return output;
}

// Keep the generated file in the same format as the checked-in TypeScript.
const { code: output, errors } = await format(outputPath, await generate());
if (errors.length > 0) throw new Error("Failed to format generated content manifest");
if (process.argv.includes("--check")) {
  const existing = await readFile(outputPath, "utf8").catch(() => "");
  if (existing !== output) {
    throw new Error("generatedContentManifest.ts is stale; run npm run generate:content");
  }
} else {
  await writeFile(outputPath, output);
}
