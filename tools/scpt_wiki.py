"""Render maintained strategy guides together with pinned upstream research."""
import re
from collections import defaultdict


def component_index(pipe, nodes):
    lines = ['# Engine component and rules index', '',
             'Rules text, card effects and numeric properties below are reproduced from this checkout\'s content corpus, not inferred from podcast advice. **Corpus presence does not establish implemented behavior, official accuracy, or inclusion in the active setup.** Source tags and replacement IDs distinguish editions and variants; a missing field is not a rule that the value is zero or false.', '',
             '[Rules reference and scope](RULES_REFERENCE.md) · [Faction guides](FACTION_SUMMARIES.md) · [Conditional strategy connections](STRATEGY_CONNECTIONS.md)', '',
             '## Browse by component type', '', '| Type | Records | First entry |', '|---|---:|---|']
    for kind in sorted(pipe.KINDS.values()):
        members = sorted((cid, node) for cid, node in nodes.items() if cid.startswith(kind + ':'))
        if members:
            cid, node = members[0]
            lines.append(f'| {kind} | {len(members)} | [{pipe.md(node["name"])}](#{pipe.slug(cid)}) |')
    lines += ['', f'**{len(nodes)} records in {len(pipe.KINDS)} categories.** Use the stable ID or component name to search this page. ASR mentions and strategy backlinks remain explicitly draft.', '']
    return lines


def component_details(pipe, node, nodes):
    """Render only corpus fields, preserving zero/false and distinct timing windows."""
    record = node.get('details', {})
    lines = []
    attributes = ('faction', 'type', 'types', 'baseType', 'requirements', 'baseUpgrade',
                  'initiative', 'phase', 'playTiming', 'window', 'abilityWindow',
                  'tfName', 'tfAbilityWindow', 'wildWildWindow',
                  'unlockCondition', 'target', 'resolution', 'points', 'synergy',
                  'cost', 'moveValue', 'capacityValue', 'capacityUsed',
                  'combatHitsOn', 'combatDieCount', 'afbHitsOn', 'afbDieCount',
                  'bombardHitsOn', 'bombardDieCount', 'spaceCannonHitsOn', 'spaceCannonDieCount',
                  'basicProduction', 'productionValue', 'sustainDamage', 'planetaryShield',
                  'deepSpaceCannon', 'disablesPlanetaryShield', 'canBeDirectHit',
                  'isShip', 'isGroundForce', 'isStructure', 'isUpgrade', 'isPlanetOnly', 'isSpaceOnly',
                  'resources', 'influence', 'planetType', 'planetTypes', 'techSpecialties',
                  'techSpeciality', 'resourcesModifier', 'influenceModifier', 'isLegendary',
                  'isFakeAttachment', 'isFakeRelic', 'playArea', 'playImmediately',
                  'wormholes', 'tileBack', 'isAsteroidField', 'isGravityRift', 'isHyperlane',
                  'isNebula', 'isScar', 'isSupernova', 'complexity')
    present = [(key, record[key]) for key in attributes if key in record and record[key] is not None]
    if present:
        lines += ['| Corpus field | Value |', '|---|---|']
        for key, value in present:
            rendered = pipe.md(str(value).lower() if isinstance(value, bool) else ', '.join(map(str, value)) if isinstance(value, list) else value)
            lines.append(f'| `{key}` | {rendered.replace(chr(10), "<br>")} |')
        lines.append('')
    text_fields = (('text', 'Effect'), ('ability', 'Unit ability'), ('permanentEffect', 'Permanent effect'),
                   ('windowEffect', 'Window effect'), ('abilityName', 'Ability name'), ('abilityText', 'Leader ability'),
                   ('tfAbilityText', "Twilight's Fall alternate ability"),
                   ('wildWildText', 'Wild Wild Galaxy alternate effect'),
                   ('primaryTexts', 'Primary'), ('secondaryTexts', 'Secondary'),
                   ('text1', 'Outcome 1'), ('text2', 'Outcome 2'),
                   ('legendaryAbilityName', 'Legendary ability'), ('legendaryAbilityText', 'Legendary effect'),
                   ('description', 'Rule text'), ('notes', 'Corpus notes'))
    for key, label in text_fields:
        value = record.get(key)
        if value:
            # Rule descriptions are consecutive fragments, while card effects are separate items.
            if isinstance(value, list):
                value = ('' if key == 'description' else '\n\n').join(str(v) for v in value)
            lines += [f'**{label} (corpus):**', '', pipe.md(value), '']
    for field in record.get('fields', []):
        value = field.get('text', [])
        text = ''.join(value) if isinstance(value, list) else str(value)
        lines += [f'**{pipe.md(field.get("title", "Rule detail"))} (corpus):**', '', pipe.md(text), '']
    references = [('faction', 'faction'), ('factionHomeworld', 'faction'), ('tileId', 'system'),
                  ('planets', 'planet'), ('attachmentId', 'attachment'), ('requiredTechId', 'technology'),
                  ('upgradesFromUnitId', 'unit'), ('upgradesToUnitId', 'unit'),
                  ('homebrewReplacesID', node['id'].split(':')[0])]
    for field, kind in references:
        values = record.get(field, [])
        if not isinstance(values, list):
            values = [values]
        for value in values:
            target = f'{kind}:{value}'
            if target not in nodes:
                if value:
                    lines += [f'- `{field}`: `{pipe.md(value)}` (no indexed target)']
                continue
            destination = f'factions/{value}.md' if kind == 'faction' else '#' + pipe.slug(target)
            lines += [f'- `{field}`: [{pipe.md(nodes[target]["name"])}]({destination}) (`{target}`)']
    return lines + ['']


def rules_reference(pipe, nodes):
    lines = ['# Rules reference and corpus scope', '',
             '[Component text and statistics](components.md) · [Wiki](README.md)', '',
             '## Authority and versions', '',
             'Use the [publisher rules and FAQ downloads](https://www.fantasyflightgames.com/en/products/twilight-imperium-fourth-edition/) for official rulebooks, expansion rules and Codex material. Apply the documents matching the game setup. This page indexes local content; it does not certify the corpus against those documents or against engine execution.', '',
             'The local `rules.json` contains eight Thunder\'s Edge topics, not a complete base-game or Prophecy of Kings rules reference. It also contains explanatory notes, examples, tips and bot-specific notation; those are retained as corpus material rather than promoted to official rulings. General movement, combat, production, transactions and scoring still require the applicable rulebook.', '',
             '## Local rule topics', '', '| Topic | Source | Text |', '|---|---|---|']
    for cid, node in sorted(nodes.items()):
        if cid.startswith('rule:'):
            lines.append(f'| {pipe.md(node["name"])} | `{node["source"]}` | [Corpus text](components.md#{pipe.slug(cid)}) |')
    lines += ['', '## Read component rules in context', '',
              '- Strategy cards expose primary and secondary effects separately; these are different routes and costs.',
              '- Leaders expose ability windows and unlock conditions alongside their effects.',
              '- Units expose movement, capacity, production, combat dice and abilities only where the record supplies them. Printed unit statistics do not describe the complete tactical-action sequence.',
              '- Technologies retain prerequisite strings and unit-upgrade references; this page does not resolve research legality.',
              '- Agendas preserve both outcome texts. Exploration cards and attachments remain separate records.',
              '- Planets expose printed resources, influence, specialties and legendary effects. Systems link their planets and retain anomaly/wormhole fields.',
              '- Breakthroughs and galactic events retain their source tags; their inclusion here does not activate expansion or optional rules.', '',
              '## Evidence boundaries', '',
              'A direct component link means an explicit corpus field matched a known ID. A faction ownership/start link means faction content declared it. An ASR mention is a text-match candidate. A strategy connection is conditional draft advice with citations. None of these alone proves that an engine action is legal or that an effect is implemented.', '']
    pipe.generated(pipe.WIKI / 'RULES_REFERENCE.md', '\n'.join(lines))


def graph_view(pipe, graph, catalog):
    """Build an offline viewer with explicit corpus links and separately labeled draft edges."""
    import json
    nodes = graph['nodes']
    fields = ('faction', 'type', 'types', 'requirements', 'initiative', 'phase', 'window',
              'playTiming', 'abilityWindow', 'unlockCondition', 'synergy', 'points', 'cost',
              'moveValue', 'capacityValue', 'combatHitsOn', 'combatDieCount',
              'productionValue', 'sustainDamage', 'planetaryShield', 'resources', 'influence',
              'planetType', 'techSpecialties', 'wormholes', 'isAsteroidField', 'isGravityRift',
              'isNebula', 'isSupernova', 'isScar', 'tfAbilityWindow', 'wildWildWindow')
    texts = (('text', 'Effect'), ('ability', 'Unit ability'), ('permanentEffect', 'Permanent effect'),
             ('windowEffect', 'Window effect'), ('abilityText', 'Leader ability'),
             ('primaryTexts', 'Primary'), ('secondaryTexts', 'Secondary'),
             ('text1', 'Outcome 1'), ('text2', 'Outcome 2'),
             ('legendaryAbilityText', 'Legendary effect'), ('description', 'Rule text'),
             ('tfAbilityText', "Twilight's Fall alternate ability"),
             ('wildWildText', 'Wild Wild Galaxy alternate effect'), ('notes', 'Corpus notes'))
    projected = []
    for cid, node in sorted(nodes.items()):
        kind = cid.split(':')[0]
        record = node.get('details', {})
        properties = [(field, ', '.join(map(str, record[field])) if isinstance(record[field], list) else record[field])
                      for field in fields if field in record and record[field] is not None]
        prose = []
        for field, label in texts:
            value = record.get(field)
            if value:
                if isinstance(value, list):
                    value = ('' if field == 'description' else '\n\n').join(map(str, value))
                prose.append((label, value))
        for section in record.get('fields', []):
            value = section.get('text', [])
            prose.append((section.get('title', 'Rule detail'), ''.join(value) if isinstance(value, list) else str(value)))
        projected.append({'id': cid, 'name': node['name'], 'kind': kind, 'source': node.get('source'),
                          'page': f'factions/{cid[8:]}.md' if kind == 'faction' else 'components.md#' + pipe.slug(cid),
                          'fields': properties, 'texts': prose})
    episodes = {episode['id']: episode for episode in catalog['episodes']}
    links = []
    for edge in graph['edges']:
        group = 'fact' if edge['provenance'] == 'engine-content' else 'strategy' if edge['provenance'] == 'model-summary draft' else 'mention'
        item = {'from': edge['from'], 'to': edge['to'], 'relation': edge['relation'], 'group': group}
        if edge.get('conditions'):
            item['conditions'] = edge['conditions']
        if edge.get('faction'):
            item['context'] = f'factions/{edge["faction"]}.md#strategy-connections'
        citations = edge.get('citations', [])
        if edge.get('episode'):
            citations = [{'kind': 'episode', 'episode': edge['episode'], 'start': edge['start'], 'end': edge['end']}]
        item['citations'] = []
        for cite in citations:
            if cite['kind'] == 'episode':
                episode = episodes[cite['episode']]
                item['citations'].append({'label': f'SCPT {episode["number"]} · {cite["start"]:.0f}–{cite["end"]:.0f}s', 'url': episode['url']})
            elif cite['kind'] == 'document':
                item['citations'].append({'label': cite['heading'], 'url': f'sources/{cite["document"]}.md#' + github_anchor(cite['heading'])})
        links.append(item)
    # These are factual ID references, not inferred synergies or runtime claims.
    references = [('faction', 'faction'), ('factionHomeworld', 'faction'), ('tileId', 'system'),
                  ('planets', 'planet'), ('attachmentId', 'attachment'), ('requiredTechId', 'technology'),
                  ('upgradesFromUnitId', 'unit'), ('upgradesToUnitId', 'unit'),
                  ('homebrewReplacesID', None)]
    for cid, node in sorted(nodes.items()):
        for field, kind in references:
            values = node.get('details', {}).get(field, [])
            if not isinstance(values, list):
                values = [values]
            for value in values:
                target = f'{kind or cid.split(":")[0]}:{value}'
                if target in nodes:
                    links.append({'from': cid, 'to': target, 'relation': field, 'group': 'fact'})
    payload = json.dumps({'nodes': projected, 'edges': links}, ensure_ascii=False, separators=(',', ':')).replace('<', '\\u003c')
    template = (pipe.ROOT / 'tools/scpt_graph.html').read_text(encoding='utf-8')
    if template.count('__WIKI_GRAPH_DATA__') != 1:
        raise ValueError('Graph template requires exactly one data placeholder')
    pipe.generated(pipe.WIKI / 'graph-view.html', template.replace('__WIKI_GRAPH_DATA__', payload))
    return {'nodes': len(projected), 'edges': len(links)}


def sections(text):
    """Keep exact headings and stop at the next heading of equal/higher level."""
    lines = text.splitlines()
    result = []
    for i, line in enumerate(lines):
        match = re.match(r'^(#{1,6})\s+(.+?)\s*$', line)
        if not match:
            continue
        level, heading = len(match[1]), match[2]
        end = len(lines)
        for j in range(i + 1, len(lines)):
            following = re.match(r'^(#{1,6})\s+', lines[j])
            if following and len(following[1]) <= level:
                end = j
                break
        result.append((heading, '\n'.join(lines[i + 1:end]).strip()))
    return result


def family(fid):
    if fid in ('keleresm', 'keleresx', 'keleresa'):
        return 'keleresm'
    return 'firmament' if fid == 'obsidian' else fid


def heading_matches(heading, fid, aliases, normalize):
    if '`' in heading:
        codes = re.findall(r'`([^`]+)`', heading)
        return fid in codes or (fid.startswith('keleres') and 'keleres' in codes)
    if fid.startswith('keleres'):
        terms = ['keleres']
    elif fid in ('firmament', 'obsidian'):
        terms = ['firmament', 'obsidian']
    else:
        terms = aliases.get(fid, [fid])
    return any(re.search(r'(?<!\w)' + re.escape(normalize(term)) + r'(?!\w)', normalize(heading)) for term in terms)


def citation_links(pipe, citations, catalog, refs, prefix=''):
    links = []
    for cite in citations:
        if cite['kind'] == 'episode':
            ep = next(e for e in catalog['episodes'] if e['id'] == cite['episode'])
            links.append(f"[SCPT {ep['number']}, {cite['start']:.2f}–{cite['end']:.2f}s]({ep['url']})")
        else:
            ref = next(r for r in refs if r['id'] == cite['document'])
            links.append(f"[{pipe.md(cite['heading'])}]({prefix}sources/{ref['id']}.md#{github_anchor(cite['heading'])})")
    return '; '.join(links)


def github_anchor(heading):
    # Markdown heading fragments retain underscores and collapse spaces to hyphens.
    return re.sub(r'[^\w\- ]', '', heading.lower()).replace(' ', '-')


def integrate(pipe, catalog, refs, factions, edges):
    documents = {}
    for ref in refs:
        path = pipe.WIKI / 'sources' / (ref['id'] + '.md')
        if not path.exists() or pipe.digest_file(path) != ref['sha256']:
            raise ValueError('Missing or changed maintained upstream snapshot: ' + ref['id'])
        documents[ref['id']] = sections(path.read_text(encoding='utf-8'))
    nodes = pipe.graph_nodes()
    summaries = {}
    comparisons_path = pipe.WIKI / 'source_comparisons.json'
    comparisons = pipe.read(comparisons_path) if comparisons_path.exists() else {}
    backlinks = defaultdict(list)
    strategy_edges = defaultdict(list)
    for edge in edges:
        if edge['provenance'] == 'model-summary draft':
            strategy_edges[edge['from']].append(edge)
            if edge['to'] != edge['from']:
                strategy_edges[edge['to']].append(edge)
    for faction in factions:
        fid = faction['alias']
        path = pipe.WIKI / 'summaries' / (fid + '.json')
        if path.exists():
            summaries[fid] = pipe.read(path)
            for claim in summaries[fid]['claims']:
                for component in set(claim['components']):
                    backlinks[component].append((fid, claim['id']))
    families = {}
    for faction in factions:
        fid = faction['alias']
        if fid in summaries:
            families.setdefault(family(fid), []).append(fid)
    unique_claims = {(family(fid), c['id']) for fid, summary in summaries.items() for c in summary['claims']}
    source_edge_count = sum(len(summaries[fid]['connections']) for fid in families)
    collection = ['# Faction strategy summary collection', '',
                  f"Complete draft collection: **{len(summaries)} faction IDs / {len(families)} guides / {len(unique_claims)} distinct claims / {source_edge_count} source-family connections**.", '',
                  'Readable guides are maintained here alongside the structured JSON. Faction pages combine them with pinned upstream context and source comparisons. Shared variants retain their explicit form/starting conditions; current rules and ASR accuracy remain unreviewed.', '',
                  '| Faction / source family | Integrated wiki page | Readable guide | Structured drafts |', '|---|---|---|---|']
    connections = ['# Conditional strategy connections', '',
                   'Source-backed draft interactions, grouped by guide family. Conditions, source era and uncertainty still apply. [Wiki](README.md) · [Guide collection](FACTION_SUMMARIES.md) · [Components and backlinks](components.md)', '']
    for fid, variants in families.items():
        name = next(f['factionName'] for f in factions if f['alias'] == fid)
        collection.append(f"| {pipe.md(name)} | " + ' / '.join(f'[{v}](factions/{v}.md)' for v in variants) + f" | [Guide](summaries/{fid}-strategy-summary.md) | " + ' / '.join(f'[{v}](summaries/{v}.json)' for v in variants) + ' |')
        connections += [f'## {pipe.md(name)}', '', f'[Integrated page](factions/{fid}.md) · [Readable guide](summaries/{fid}-strategy-summary.md)', '']
        for edge in summaries[fid]['connections']:
            def link(target):
                destination = f'factions/{target[8:]}.md' if target.startswith('faction:') else 'components.md#' + pipe.slug(target)
                return f"[{pipe.md(nodes[target]['name'])}]({destination})"
            connections.append(f"- {link(edge['from'])} **{edge['relation']}** {link(edge['to'])}. {pipe.md(edge['conditions'])} Sources: {citation_links(pipe, edge['citations'], catalog, refs)}")
        connections.append('')
    collection += ['', '[Strategy connections](STRATEGY_CONNECTIONS.md) · [Pinned upstream sources](UPSTREAM.md) · [Summary contract](SUMMARY_CONTRACT.md)', '',
                   'Earlier Hacan, Letnev and Sol drafts and the Pi Winnu backup are retained in the local extraction archive; they are not required to navigate this wiki.', '']
    pipe.generated(pipe.WIKI / 'FACTION_SUMMARIES.md', '\n'.join(collection))
    pipe.generated(pipe.WIKI / 'STRATEGY_CONNECTIONS.md', '\n'.join(connections))
    for faction in factions:
        fid = faction['alias']
        path = pipe.WIKI / 'factions' / (fid + '.md')
        original = path.read_text(encoding='utf-8')
        guidepath = pipe.WIKI / 'summaries' / (family(fid) + '-strategy-summary.md')
        if fid not in summaries or not guidepath.exists():
            continue  # Discovery/transcription runs may precede summarization.
        guide = guidepath.read_text(encoding='utf-8')
        guide = re.sub(r'^# [^\n]+\n', '', guide, count=1).strip()
        # Imported guides use publisher links; rebase any relative Markdown links.
        guide = re.sub(r'(\]\()((?!https?://|#)[^)]+)(\))', r'\1../summaries/\2\3', guide)
        guide = re.sub(r'^(#{2,5}) ', r'\1# ', guide, flags=re.M)
        matched = [(ref, heading, body) for ref in refs for heading, body in documents[ref['id']]
                   if heading_matches(heading, fid, pipe.ALIASES, pipe.norm)]
        if fid in ('bastion', 'deepwrought', 'firmament', 'obsidian', 'crimson', 'ralnel'):
            community = next((ref for ref in refs if ref['id'] == 'upstream-community'), None)
            if community and not any(ref['id'] == community['id'] for ref, _, _ in matched):
                matched.extend((community, heading, body) for heading, body in documents[community['id']]
                               if heading.startswith("Appendix: Thunder's Edge"))
        if not any(ref['id'] == 'upstream-plans' for ref, _, _ in matched):
            raise ValueError('Missing upstream faction profile: ' + fid)
        nav = f"[Wiki](../README.md) · [Readable guide](../summaries/{guidepath.name}) · [Structured claims](../summaries/{fid}.json) · [Manual notes](../notes/{fid}.md)"
        comparison = ['', '## Combined source guidance', '',
                      'The guide above contains conditional, transcript-cited advice. The pinned documents below add community research and a bot-oriented mid/late-game profile. Their ordered technology/build lists are proposals, not unconditional policy targets. Keep source disagreements and expansion assumptions visible.', '',
                      '**Historical scope:** upstream statements about available factions, engine support, bot behavior and benchmark results describe that source revision; they are not checks of this checkout. Podcast timestamps in upstream notes are approximate and do not share the local transcript verification.', '']
        for item in comparisons.get(family(fid), []):
            claim_ids = {c['id'] for c in summaries[fid]['claims']}
            # Shared variants may omit a claim applying only to the other form.
            targets = item['claims']
            target_fid = fid if all(cid in claim_ids for cid in targets) else family(fid)
            target_claims = {c['id'] for c in summaries[target_fid]['claims']}
            if not all(cid in target_claims for cid in targets):
                raise ValueError('Comparison references a missing claim: ' + fid)
            ref = next(r for r in refs if r['id'] == item['document'])
            if item['heading'] not in ref['headings']:
                raise ValueError('Comparison references a missing upstream heading')
            claim_links = ', '.join(f'[{cid}]({target_fid}.md#claim-{cid.lower()})' for cid in targets)
            comparison.append(f"- **{pipe.md(item['kind'])}:** {item['text']} Evidence: {claim_links}; [{pipe.md(item['heading'])}](../sources/{ref['id']}.md#{github_anchor(item['heading'])}).")
        comparison += ['', '### Upstream recommendations and context', '']
        for ref, heading, body in matched:
            local = f"../sources/{ref['id']}.md#{github_anchor(heading)}"
            comparison += [f'<details><summary>{pipe.md(ref["id"])} — {pipe.md(heading)}</summary>', '',
                           f'[Exact pinned section]({local}) · [Repository source]({ref["url"]}#{github_anchor(heading)})', '',
                           '> Historical source excerpt; advice and engine claims retain the source’s assumptions.', '', body, '', '</details>', '']
        relations = ['', '## Strategy connections', '']
        for edge in summaries[fid]['connections']:
            def component_link(cid):
                destination = f"{cid[8:]}.md" if cid.startswith('faction:') else f"../components.md#{pipe.slug(cid)}"
                return f"[{pipe.md(nodes[cid]['name'])}]({destination})"
            relations += [f"- {component_link(edge['from'])} **{edge['relation']}** {component_link(edge['to'])}. {pipe.md(edge['conditions'])} Sources: {citation_links(pipe, edge['citations'], catalog, refs, '../')}"]
        split = original.index('## Component connections')
        head, tail = original[:split], original[split:]
        tail = tail.replace('## Strategy claims', '<details><summary>Structured claims, conditions and source citations</summary>\n\n## Strategy claims')
        # The notes footer belongs outside the structured-claim disclosure.
        tail = tail.replace('\n[Manual notes]', '\n</details>\n\n[Manual notes]')
        pipe.generated(path, (head.removeprefix(pipe.MARKER).lstrip() + nav + '\n\n## Strategy guide\n\n' + guide + '\n' + '\n'.join(comparison + relations) + '\n\n' + tail))
    componentpath = pipe.WIKI / 'components.md'
    text = componentpath.read_text(encoding='utf-8')
    chunks = re.split(r'(?=<a id=")', text)
    rendered = [chunks[0].removeprefix(pipe.MARKER).lstrip()]
    for chunk in chunks[1:]:
        anchor = re.match(r'<a id="([^"]+)">', chunk)[1]
        cid = next(cid for cid in nodes if pipe.slug(cid) == anchor)
        if backlinks[cid]:
            chunk += '\nStrategy claim backlinks: ' + ', '.join(f'[{fid} {claim}](factions/{fid}.md#claim-{claim.lower()})' for fid, claim in sorted(backlinks[cid])) + '.\n'
        for edge in strategy_edges[cid]:
            def target_link(target):
                destination = f'factions/{target[8:]}.md' if target.startswith('faction:') else '#' + pipe.slug(target)
                return f"[{pipe.md(nodes[target]['name'])}]({destination})"
            chunk += f"\n- **Strategy draft:** {target_link(edge['from'])} {edge['relation']} {target_link(edge['to'])}; [{edge['faction']} context](factions/{edge['faction']}.md#strategy-connections). {pipe.md(edge['conditions'])} Sources: {citation_links(pipe, edge['citations'], catalog, refs)}\n"
        rendered.append(chunk)
    pipe.generated(componentpath, '\n'.join(rendered))
    indexpath = pipe.WIKI / 'README.md'
    text = indexpath.read_text(encoding='utf-8').removeprefix(pipe.MARKER).lstrip()
    text = text.replace('## Factions', '[Human repo guide](REPO_GUIDE.md) · [Wiki procedure](SCHEMA.md) · [Maintenance log](LOG.md)\n\n## Factions', 1)
    text = text.replace('## Factions', '## Read the strategy wiki\n\n[Interactive graph view](graph-view.html) · [Components, card text and statistics](components.md) · [Rules reference and scope](RULES_REFERENCE.md)\n\n[Readable guide collection](FACTION_SUMMARIES.md) · [Strategy connections](STRATEGY_CONNECTIONS.md) · [Pinned upstream research](UPSTREAM.md)\n\n[Wiki-to-MLP research and measured follow-ups](MLP_APPLICATIONS.md)\n\nFaction pages combine the maintained readable guide, source comparisons, historical upstream profiles, conditional component interactions and structured evidence. Component pages link back to individual claims and both sides of strategy connections.\n\n## Factions', 1)
    pipe.generated(indexpath, text)
