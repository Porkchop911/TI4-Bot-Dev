from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch
from datetime import datetime,timezone
import scpt_strategy as pipe
import scpt_wiki

class StrategyTests(unittest.TestCase):
    def test_offline_graph_keeps_isolated_nodes_provenance_and_safe_embedded_text(self):
        graph = {'nodes': {
            'rule:alone': {'name': 'An isolated rule', 'source': 'test', 'details': {'description': ['Safe </script><script>bad</script> text']}},
            'planet:p': {'name': 'Planet', 'source': 'test', 'details': {'tileId': 's'}},
            'system:s': {'name': 'System', 'source': 'test', 'details': {'planets': ['p']}}},
            'edges': [
                {'from': 'planet:p', 'to': 'system:s', 'relation': 'draft', 'provenance': 'model-summary draft', 'conditions': 'Only sometimes'},
                {'from': 'planet:p', 'to': 'system:s', 'relation': 'mention', 'provenance': 'ASR candidate'}]}
        with patch.object(pipe, 'generated') as save:
            counts = scpt_wiki.graph_view(pipe, graph, {'episodes': []})
        page = save.call_args.args[1]
        raw = re.search(r'<script id="wiki-data" type="application/json">(.*?)</script>', page, re.S)[1]
        self.assertNotIn('</script>', raw)
        payload = json.loads(raw)
        self.assertEqual(counts['nodes'], 3)
        self.assertEqual({node['id'] for node in payload['nodes']}, set(graph['nodes']))
        self.assertEqual({edge['group'] for edge in payload['edges']}, {'fact', 'strategy', 'mention'})
        self.assertTrue(any(edge['relation'] == 'tileId' for edge in payload['edges']))
        self.assertTrue(any(edge['relation'] == 'planets' for edge in payload['edges']))
        isolated = next(node for node in payload['nodes'] if node['id'] == 'rule:alone')
        self.assertEqual(isolated['texts'][0][1], 'Safe </script><script>bad</script> text')
        self.assertNotIn('__WIKI_GRAPH_DATA__', page)

    def test_component_details_preserve_windows_zero_false_and_separate_card_effects(self):
        node = {'id': 'strategy-card:test', 'details': {'initiative': 0, 'playImmediately': False,
                'window': 'AFTER <test>', 'primaryTexts': ['Primary one.', 'Primary two.'],
                'secondaryTexts': ['Pay a token.']}}
        text = '\n'.join(scpt_wiki.component_details(pipe, node, {}))
        self.assertIn('| `initiative` | 0 |', text)
        self.assertIn('| `playImmediately` | false |', text)
        self.assertIn('AFTER &lt;test&gt;', text)
        self.assertLess(text.index('**Primary (corpus):**'), text.index('Primary one.'))
        self.assertLess(text.index('**Secondary (corpus):**'), text.index('Pay a token.'))

    def test_component_rule_fragments_and_nested_clarifications_are_not_lost(self):
        node = {'id': 'rule:test', 'details': {'description': ['First ', 'sentence.'],
                'fields': [{'title': 'Combat', 'text': ['Separate ', 'combat.']}]}}
        text = '\n'.join(scpt_wiki.component_details(pipe, node, {}))
        self.assertIn('First sentence.', text)
        self.assertIn('**Combat (corpus):**\n\nSeparate combat.', text)

    def test_component_references_link_known_ids_without_fabricating_targets(self):
        node = {'id': 'unit:test', 'details': {'faction': 'sol', 'requiredTechId': 'missing',
                'upgradesToUnitId': 'test2'}}
        nodes = {'faction:sol': {'name': 'Sol'}, 'unit:test2': {'name': 'Upgraded test'}}
        text = '\n'.join(scpt_wiki.component_details(pipe, node, nodes))
        self.assertIn('[Sol](factions/sol.md)', text)
        self.assertIn('[Upgraded test](#unit-test2)', text)
        self.assertIn('`missing` (no indexed target)', text)
        self.assertNotIn('#technology-missing', text)

    def test_exploration_attachment_and_alternate_leader_rules_retain_context(self):
        nodes = pipe.graph_nodes()
        text = '\n'.join(scpt_wiki.component_details(pipe, nodes['exploration:dmz'], nodes))
        self.assertIn('](#attachment-dmz)', text)
        node = {'id': 'leader:test', 'details': {'abilityText': 'Ordinary effect.',
                'tfAbilityWindow': 'ACTION:', 'tfAbilityText': 'Alternate effect.'}}
        text = '\n'.join(scpt_wiki.component_details(pipe, node, nodes))
        self.assertIn('**Leader ability (corpus):**\n\nOrdinary effect.', text)
        self.assertIn("**Twilight's Fall alternate ability (corpus):**", text)
        self.assertIn('`tfAbilityWindow` | ACTION:', text)

    def test_expanded_corpus_includes_rules_names_stats_and_all_new_categories(self):
        nodes = pipe.components()
        self.assertTrue({'agenda', 'breakthrough', 'exploration', 'attachment', 'planet', 'system',
                         'galactic-event', 'rule'}.issubset({cid.split(':')[0] for cid in nodes}))
        self.assertEqual(nodes['rule:coexist']['name'], 'Coexist')
        self.assertTrue(nodes['rule:coexist']['details']['fields'])
        self.assertEqual(nodes['planet:0.0.0']['details']['resources'], 5)
        self.assertEqual(nodes['planet:0.0.0']['details']['influence'], 0)

    def test_upstream_sections_keep_nested_context_and_exact_headings(self):
        source = '# Research\n## Factions\n### The Sol (`sol`)\nAdvice.\n#### Caveat\nConditional.\n### The Hacan (`hacan`)\nOther.\n'
        parsed = dict(scpt_wiki.sections(source))
        self.assertEqual(parsed['The Sol (`sol`)'], 'Advice.\n#### Caveat\nConditional.')
        self.assertNotIn('Other.', parsed['The Sol (`sol`)'])

    def test_upstream_heading_matching_does_not_merge_shared_variant_states(self):
        heading = 'The Firmament / The Obsidian (`firmament`, `obsidian`)'
        self.assertTrue(scpt_wiki.heading_matches(heading, 'obsidian', pipe.ALIASES, pipe.norm))
        self.assertFalse(scpt_wiki.heading_matches(heading, 'sol', pipe.ALIASES, pipe.norm))
        self.assertFalse(scpt_wiki.heading_matches('Solace — notes', 'sol', pipe.ALIASES, pipe.norm))
        self.assertEqual(scpt_wiki.family('obsidian'), 'firmament')
        self.assertEqual(scpt_wiki.family('keleresx'), 'keleresm')

    def test_build_cache_revalidates_transcripts_in_a_new_build_context(self):
        episode = {'id': 'e'}
        catalog = {'episodes': [episode]}
        cite = {'kind': 'episode', 'episode': 'e', 'start': 1.0, 'end': 2.0}
        transcript = {'complete': True, 'duration_seconds': 3, 'segments': [{'start': 1, 'end': 2, 'text': 'Speech'}]}
        with patch.object(pipe, 'transcript_for', return_value=transcript) as load:
            cache = {}
            pipe.citation_ok(cite, catalog, [], cache)
            pipe.citation_ok(cite, catalog, [], cache)
            self.assertEqual(load.call_count, 1)
            pipe.citation_ok(cite, catalog, [], {})
            self.assertEqual(load.call_count, 2)

    def feed(self, entries):
        return ('<rss><channel>' + ''.join(f'<item><title>{title}</title><guid>{gid}</guid><pubDate>{date}</pubDate><link>https://spacecatspeaceturtles.podbean.com/e/{gid}/</link><enclosure url="https://pscrb.fm/rss/p/mcdn.podbean.com/{gid}.mp3" length="10" /></item>' for title,gid,date in entries) + '</channel></rss>').encode()

    def test_latest_guide_is_by_publication_not_feed_order_or_latest_mention(self):
        factions=[{'alias':'hacan','factionName':'The Emirates of Hacan'}]
        entries=[('10: Hacan Strategy Guide','old','Wed, 01 Jan 2025 00:00:00 +0000'),('12: Hacan Tier List','newmention','Fri, 02 Jan 2026 00:00:00 +0000'),('11: Super Emirates of Hacan Guide','newguide','Thu, 02 Jan 2025 00:00:00 +0000'),('13: Ultimate Hacan Guide','future','Fri, 01 Jan 2027 00:00:00 +0000')]
        c=pipe.parse_feed(self.feed(entries),factions,datetime(2026,10,14,tzinfo=timezone.utc))
        self.assertEqual([e['guid'] for e in c['episodes']],['newguide'])
        self.assertEqual(c['missing'],[])

    def test_shared_guides_deduplicate_engine_variants(self):
        factions=[{'alias':f,'factionName':f} for f in ['keleresm','keleresx','keleresa','firmament','obsidian']]
        c=pipe.parse_feed(self.feed([('1: Council Keleres Guide','kel','Wed, 01 Jan 2025 00:00:00 +0000'),("2: Firmament/Obsidian Beginner Guide",'fo','Thu, 02 Jan 2025 00:00:00 +0000')]),factions,datetime(2026,10,14,tzinfo=timezone.utc))
        self.assertEqual(len(c['episodes']),2)
        self.assertEqual(len(c['choices']),5)
        self.assertEqual(c['choices']['firmament'],c['choices']['obsidian'])
        self.assertEqual(c['choices']['keleresm'],c['choices']['keleresa'])

    def test_tied_newest_guides_require_review(self):
        c=pipe.parse_feed(self.feed([('1: Hacan Guide','one','Wed, 01 Jan 2025 00:00:00 +0000'),('2: Hacan Guide','two','Wed, 01 Jan 2025 00:00:00 +0000')]),[{'alias':'hacan','factionName':'Hacan'}],datetime(2026,10,14,tzinfo=timezone.utc))
        self.assertEqual(len(c['ambiguities']['hacan']),2)

    def test_readable_transcript_merges_segments_without_rewriting(self):
        segments=[{'start':i*2,'end':i*2+1,'text':f'Phrase number {i}.'} for i in range(100)]
        paragraphs=pipe.transcript_paragraphs(segments)
        self.assertLess(len(paragraphs),10)
        self.assertEqual(' '.join(t for _,t in paragraphs),' '.join(s['text'] for s in segments))
        self.assertEqual(paragraphs[0][0],0)
        with tempfile.TemporaryDirectory() as name:
            folder=Path(name)
            pipe.write_readable_transcript({'title':'Test guide','url':'https://example.com/episode'}, {'complete':True,'segments':segments},folder)
            self.assertIn('Phrase number 99.',(folder/'transcript.txt').read_text())
            self.assertIn('[00:00:00]',(folder/'transcript.md').read_text())
            (folder/'transcript.txt').write_text('Manual corrections')
            with self.assertRaises(ValueError):pipe.write_readable_transcript({'title':'Test','url':'url'},{'complete':True,'segments':segments},folder)

    def test_mixed_queue_assigns_each_episode_once_with_pool_settings(self):
        episodes=[{'id':str(i)} for i in range(20)]
        def task(e, settings, pilot, runid): return (e['id'],settings['device'])
        with ThreadPoolExecutor(2) as gpu, ThreadPoolExecutor(3) as cpu:
            pools=[(gpu,2,{'device':'cuda'}),(cpu,3,{'device':'cpu'})]
            with patch.object(pipe,'transcribe_one',side_effect=task),patch.object(pipe,'stopped',return_value=False):
                results=[f.result() for e,s,f in pipe.dispatch_jobs(pools,episodes,False,'test')]
            self.assertEqual(sorted(int(i) for i,d in results),list(range(20)))
            self.assertEqual({d for i,d in results},{'cpu','cuda'})
            with patch.object(pipe,'transcribe_one',side_effect=task),patch.object(pipe,'stopped',return_value=True):
                results=list(pipe.dispatch_jobs(pools,episodes,False,'test'))
            self.assertEqual(len(results),5)

    def test_episode_folder_names_are_readable_safe_and_distinct(self):
        e={'id':'scpt-476-abc123','title':'476: Empyrean Super Guide / Notes?'}
        p=pipe.episode_folder(e)
        self.assertEqual(p.name,'476-empyrean-super-guide-notes--scpt-476-abc123')
        self.assertEqual(p.parent,pipe.DATA/'episodes')
        self.assertNotEqual(p,pipe.episode_folder({**e,'id':'scpt-476-def456'}))
        self.assertLessEqual(len(pipe.episode_folder({**e,'title':'Long title '*100}).name),90)
        with self.assertRaises(ValueError):pipe.episode_folder({**e,'id':'../escape'})

    def test_untrusted_urls_are_rejected_before_network(self):
        for url in ['file:///tmp/audio','https://127.0.0.1/audio','http://mcdn.podbean.com/a','https://u:p@mcdn.podbean.com/a','https://podbean.com.evil.example/a']:
            with self.assertRaises(ValueError):pipe.trusted(url)

    def test_manual_wiki_pages_are_preserved(self):
        with tempfile.TemporaryDirectory() as name:
            p=Path(name)/'notes.md';p.write_text('Manual source notes',encoding='utf-8')
            with self.assertRaises(ValueError):pipe.generated(p,'New text')
            self.assertEqual(p.read_text(encoding='utf-8'),'Manual source notes')

    def test_summary_requires_matching_faction_and_actual_citations(self):
        catalog={'choices':{'sol':'e','arborec':'a'},'episodes':[]}
        claim={'statement':'Draft statement','conditions':'Conditions','compatibility':'Unknown','uncertainty':'Unknown','components':['technology:sl'],'citations':[{'kind':'document','document':'upstream-plans','heading':'General rules'}]}
        s={'schema':1,'status':'draft','summarizer':'local','faction':'sol','claims':[claim],'connections':[]}
        refs=[{'id':'upstream-plans','headings':['General rules']}]
        pipe.validate_summary(s,catalog,{'technology:sl':{}},refs,'sol')
        with self.assertRaises(ValueError):pipe.validate_summary(s,catalog,{'technology:sl':{}},refs,'arborec')
        s['claims'][0]['citations'][0]['heading']='Invented heading'
        with self.assertRaises(ValueError):pipe.validate_summary(s,catalog,{'technology:sl':{}},refs,'sol')

    def test_graph_nodes_are_the_same_ids_the_validator_accepts(self):
        nodes=pipe.graph_nodes()
        self.assertIn('faction:winnu',nodes)
        self.assertEqual(set(nodes)-set(pipe.components()),{k for k in nodes if k.startswith('faction:')})
        catalog={'choices':{'winnu':'e'},'episodes':[]}
        claim={'statement':'Draft statement','conditions':'Conditions','compatibility':'Unknown','uncertainty':'Unknown','components':['faction:winnu'],'citations':[{'kind':'document','document':'upstream-plans','heading':'General rules'}]}
        s={'schema':1,'status':'draft','summarizer':'local','faction':'winnu','claims':[claim],'connections':[]}
        refs=[{'id':'upstream-plans','headings':['General rules']}]
        pipe.validate_summary(s,catalog,nodes,refs,'winnu')
        with self.assertRaises(ValueError):pipe.validate_summary(s,catalog,pipe.components(),refs,'winnu')

    def test_episode_citations_reject_pilot_missing_and_out_of_range(self):
        catalog={'episodes':[{'id':'e'}]}
        c={'kind':'episode','episode':'e','start':10,'end':20}
        with patch.object(pipe,'transcript_for',return_value=None):
            with self.assertRaises(ValueError):pipe.citation_ok(c,catalog,[])
        t={'duration_seconds':100,'segments':[{'start':5,'end':25}]}
        with patch.object(pipe,'transcript_for',return_value=t):
            pipe.citation_ok(c,catalog,[])
            with self.assertRaises(ValueError):pipe.citation_ok({**c,'end':101},catalog,[])
            with self.assertRaises(ValueError):pipe.citation_ok({**c,'start':50,'end':60},catalog,[])

    def test_real_catalog_graph_and_generated_links_are_consistent(self):
        catalog=pipe.read(pipe.WIKI/'catalog.json');graph=pipe.read(pipe.WIKI/'graph.json')
        self.assertEqual(set(catalog['choices']),{f['alias'] for f in pipe.read(pipe.CONTENT/'factions.json') if f['alias']!='neutral'})
        self.assertFalse(catalog['missing']);self.assertFalse(catalog['ambiguities'])
        self.assertEqual(set(graph['nodes']),set(pipe.components())|set(pipe.faction_nodes()))
        for e in graph['edges']:
            self.assertIn(e['from'],graph['nodes']);self.assertIn(e['to'],graph['nodes'])
        anchors=[pipe.slug(k) for k in pipe.components()]
        self.assertEqual(len(anchors),len(set(anchors)))
        for p in pipe.WIKI.rglob('*.md'):
            if p.name in ['SUMMARY_CONTRACT.md','PIPELINE.md']:continue
            for target in re.findall(r'\]\(([^)]+)\)',p.read_text(encoding='utf-8')):
                if target.startswith(('https://','http://')):continue
                self.assertNotIn('\\',target)
                dest=(p.parent/target.split('#')[0]).resolve()
                self.assertTrue(dest.exists(),f'{p}: missing {target}')

    def test_rendered_summary_claims_put_sources_after_the_claim(self):
        pages=[p for p in (pipe.WIKI/'factions').glob('*.md')]
        checked=0
        for p in pages:
            lines=p.read_text(encoding='utf-8').splitlines()
            for i,line in enumerate(lines):
                if not line.startswith('  Sources: '):continue
                checked+=1
                self.assertTrue(lines[i-2].startswith('- **Draft:**'),f'{p.name}: sources line without its claim')
                self.assertTrue(lines[i-1].startswith('  Conditions: '),f'{p.name}: missing conditions line')
                self.assertNotIn('..',lines[i-1])
        self.assertGreater(checked,0)

    def test_lock_refuses_live_owner_and_recovers_only_owned_stale_lock(self):
        with tempfile.TemporaryDirectory() as name,patch.object(pipe,'DATA',Path(name)):
            lock=pipe.DATA/'run.lock';pipe.dump(lock,{'owner':'scpt_strategy','pid':123,'runid':'old'})
            with patch.object(pipe,'alive',return_value=True):
                with self.assertRaises(ValueError):pipe.claim_lock('new')
            with patch.object(pipe,'alive',return_value=False):
                pipe.claim_lock('new')
            self.assertEqual(pipe.read(lock)['runid'],'new')
            self.assertEqual(pipe.read(lock)['pid'],pipe.os.getpid())

    def test_clock_formats_short_and_long_positions(self):
        self.assertEqual(pipe.clock(0),'00:00')
        self.assertEqual(pipe.clock(65.4),'01:05')
        self.assertEqual(pipe.clock(3725.6),'1:02:06')

    def transcript(self,complete=True):
        words=lambda s,e,items:{'start':s,'end':e,'text':'  Devotion  is best used in a large fleet  ','avg_logprob':-0.3,'compression_ratio':1.4,'no_speech_prob':0.01,'needs_review':False,'words':[{'start':s,'end':e,'word':w,'probability':p} for w,p in items]}
        return {'schema':1,'episode':'e','complete':complete,'duration_seconds':400.0,'settings':{'model':'m','device':'cpu','compute_type':'int8','language':'en','beam_size':5,'vad_filter':True,'audio_sha256':'a'*64,'pilot':False},'segments':[words(10,20,[('Devotion',0.95),('gum',0.31)]),words(310,330,[('Mechs',0.9)]).copy()|{'needs_review':True}]}

    def episode(self):
        return {'id':'e','number':1,'title':'1: Hacan Guide','url':'https://example/e','published':'2026-01-01T00:00:00+00:00','guide_kind':'guide','factions':['hacan']}

    def test_readable_render_is_deterministic_timestamped_and_flags_uncertainty(self):
        t=self.transcript()
        first=pipe.render_readable(self.episode(),t)
        self.assertEqual(first,pipe.render_readable(self.episode(),dict(t)))
        self.assertIn('[00:10](https://example/e#t=10)',first)
        self.assertIn('Devotion is best used in a large fleet',first)
        self.assertNotIn('- `00:10',first)
        self.assertIn('## 00:10 \u2013 00:20',first)
        self.assertIn('## 05:10 \u2013 05:30',first)
        self.assertIn('Flagged spans for review: 05:10–05:30',first)
        self.assertIn('"gum" at 00:10 (0.31)',first)
        self.assertIn('flagged for review 1',first)
        self.assertTrue(first.startswith(pipe.MARKER))
        with self.assertRaises(ValueError):pipe.render_readable(self.episode(),self.transcript(False))

    def test_readable_reports_missing_transcript_as_failure(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            with patch.object(pipe,'DATA',root),patch.object(pipe,'READABLE',root/'readable'):
                pipe.dump(root/'catalog.json',{'choices':{'hacan':'e'},'episodes':[self.episode()]})
                rows,failures=pipe.build_readable(None,None)
                self.assertEqual(rows,[])
                self.assertEqual(failures[0][0],'e')
                index=(root/'readable'/'index.md').read_text(encoding='utf-8')
                self.assertIn('Rendered: 0 of 1 requested episodes',index)
                self.assertIn('no complete transcript pointer',index)

if __name__=='__main__':unittest.main()
