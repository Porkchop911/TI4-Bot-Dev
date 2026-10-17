# Backward planning, Agent B's groups

Companion to `docs/BACKWARD_PLANNING.md` (Agent A: framework, Mecatol/Imperial, territory, fleet). Code in `crates/ti4-rule-bot/src/backward/`; knobs are A's (`bw_enabled`, `bw_payment`, `bw_technology`, `bw_combat`), all off by default (`bw_enabled=false`). No existing default or `for_version` table changed.

## Groups

| file | goals | what it plans | steer |
|---|---|---|---|
| `payment.rs` | spend N resources / influence / trade goods / tokens (monument, golden_age, sway_council, manipulate_law, trade_routes, centralize_trade, lead, galvanize, amass_wealth, vast_reserves) | modelled as cheap: payable now has p 0.95 (the existing `Reserve` keeps the price); a gap of at most 3 trade goods or tokens is helped by Trade or Leadership (p 0.45); planet-spend gaps are left to the default logic. `Plan.payment = true` so the framework's discount applies | `cards` |
| `technology.rs` | unit upgrades, colour pairs, one-colour runs, faction technologies (families `unit_upgrades`, `colours_2`, `same_colour_technologies`, `faction_technologies`) | smallest set (at most 3) of technologies that makes the requirement true, ordered by printed prerequisites; researches before the status phase: own Technology primary 0.95, its paid second 0.9 (6 resources), a rival's secondary 0.7 (token + 4 resources) | `cards: Technology`; `research_pick` steers the research choice itself (hooked in `objectives::pick_research`); `reach_by_research(seen, me, ids)` for prep steps |
| `combat.rs` | event secrets dts, sar, dtgs, btv, dyp (no engine progress, so `event_goals` adds them to `PlanCtx.goals`) | one-step plan over `PlanCtx.steps`: the fight that stages the event, p = fight success x event factor | `systems` |
| `production.rs` | secrets gamf (five dreadnoughts), mtm (mechs on four distinct planets), pem (production 8 in one system); knob `bw_production` (own, default off) | one production at a dock not holding the seat's command token (capacity, resources); for pem the docks Construction could add to the system with the most production | `systems`, `builds`, `cards: Construction` |
| `structures.rs` | families `structures`, `structures_away`, `units_pds`, `units_spacedock` | structures missing vs room on controlled planets vs placements from Construction (own primary 2 at 0.95, rival secondary 1 at 0.7 per strategy token) | `cards: Construction` |

Not covered, left to the default logic: the bought-objective gaps that are planet spends, secrets whose event needs a promissory note, space cannon or bombardment, faction-specific objectives.

## Measured (fixed table sol/letnev/jolnar/mentak, 4pBgaBalancedD, cyclic rotation, 160 tables = 320 games, mixed mode, candidate only the group's knobs plus `bw_enabled`)

| group | ctrl VP/round | ctrl r3 | ctrl r4 | ctrl r5 | win share |
|---|---|---|---|---|---|
| technology | +0.019 [-0.004, +0.043] | +0.016 | +0.059 [0.014, 0.104] | +0.105 [0.023, 0.187] | +2.8 pp [-0.9, +6.5] |
| combat + structures | +0.003 [-0.011, +0.018] | 0.000 | -0.006 | -0.006 | +0.9 pp [-1.3, +3.1] |
| combat + production, with `bw_prep_build` and `bw_prep_warfare` on | +0.003 [-0.013, +0.020] | -0.002 | -0.008 | -0.009 | +1.3 pp [-1.5, +4.0] |
| payment | not run: with no systems in its steer the planner changes nothing until the draft reads `steer.cards` | | | | |

Games in `/root/rulebot-results/backward-b/games/`.

## The last 13 secrets (second round of work)

| file | knob (default off) | secrets | what it does |
|---|---|---|---|
| `abilities.rs` | `bw_combat_abilities` | uf, baf, fwp, mew | one-step fight staging over `PlanCtx::steps` (plain or prepared): flagship present and surviving (0.75), a rival whose faceup note the seat holds, anti-fighter barrage dice against a fighters-only screen, bombardment dice against ground forces without planetary shield (binomial tails) |
| `agenda.rs` | `bw_agenda` | dtd, dp | odds of an election or law this round; hook `answers::pick_with`: vote For on a non-election law while holding dp with fewer than three laws. dtd needs no hook: the existing vote rule already elects the seat or its planet in every election |
| `holdings.rs` | `bw_holdings` | fsn, dhw, sb | holds back what the secret needs: no action card is played while fsn is five or fewer cards from done (menu and reaction windows), no relic purge while dhw holds two to four fragments, a rival's Support for the Throne is accepted for sb; a Politics plan for fsn |
| `ttfd.rs` | `bw_ttfd` | ttfd | a PDS on Mecatol Rex (or beside it with the Deep Space Cannon: pds2 / ht2) via the structure placement hook; odds = build x 0.3 x 0.15 |
| `unsteerable.rs` | | bam, pe | documented NONE: unsteerable, with the reason; a test keeps the list and the registry apart |

Stored games now carry `secret_ids` per seat in every snapshot; `tools/rulebot/backward_rates.py` prints per-objective score rates, unconditional and (for games with the field) conditional on having been dealt the secret.

### Measured, second round (same fixed table, 400 tables = 800 games each, `bw_enabled` plus the group's knob)

| group | ctrl VP/round | ctrl r3 / r4 / r5 | secret_objective category | win share | per-objective score rate, candidate vs baseline |
|---|---|---|---|---|---|
| abilities (+ prep build/warfare) | -0.001 | +0.001 / +0.008 / +0.004 | -0.009 | -1.1 pp | uf 4.3 vs 3.9%, baf 3.2 vs 3.3%, fwp 0.12 vs 0.12%, mew 3.6 vs 3.3% (unconditional, 1600 seat-games each) |
| agenda | +0.002 | +0.009 / +0.001 / +0.027 [0.005, 0.049] | +0.012 | -0.3 pp | dp 7.6 vs 6.7%, dtd 5.1 vs 5.0% |
| holdings | -0.001 | +0.001 / +0.001 / -0.004 | +0.003 | -0.1 pp | conditional on being dealt: fsn 54.9 vs 52.8% (+-9.9), dhw 74.5 vs 74.4%, sb 65.2 vs 65.2% |
| ttfd | +0.003 [0.000, 0.006] | 0.000 / +0.005 / +0.010 | +0.007 | +0.5 pp | conditional: 20.2 vs 17.4% (+-7.4) |

All differences are within noise except what is stated: these secrets are each dealt to about 6% of seats, so a real change of a few points in their rates would need thousands of tables (or a targeted set where the secret is dealt). The holdings hooks almost never fire (identical rates), the abilities and agenda hooks change little.

## Coverage of the corpus (80 objectives)

Generated by `backward/coverage.rs` (`cargo test -p ti4-rule-bot print_coverage_table -- --ignored --nocapture`): the 40 public and 40 secret objectives are loaded from the content store, mapped to their goal family through `objectives::goals`, and matched against the planners in dispatcher order (all group knobs on). Each planner file lists its objectives at the top and a test keeps that list equal to what the planner matches; `unsteerable::every_other_objective_has_a_planner` fails if any objective is neither planned nor listed as unsteerable. The Mecatol Rex / Imperial plan (A) is not an objective of the corpus.

**78 of 80 have a planner; 2 are documented "NONE: unsteerable".**

| alias | name | kind | pts | family | planner file |
|---|---|---|---|---|---|
| `corner` | Corner the Market | public | 1 | same_trait | territory |
| `develop` | Develop Weaponry | public | 1 | unit_upgrades | technology |
| `diversify` | Diversify Research | public | 1 | colours_2 | technology |
| `monument` | Erect a Monument | public | 1 | cost_spend_resources | payment |
| `expand_borders` | Expand Borders | public | 1 | non_home | territory |
| `research_outposts` | Found Research Outposts | public | 1 | tech_specialties | territory |
| `intimidate` | Intimidate Council | public | 1 | ships_adjacent_to_mecatol | fleet |
| `lead` | Lead From the Front | public | 1 | cost_tokens | payment |
| `trade_routes` | Negotiate Trade Routes | public | 1 | cost_trade_goods | payment |
| `sway_council` | Sway the Council | public | 1 | cost_spend_influence | payment |
| `amass_wealth` | Amass Wealth | public | 1 | cost_all_three | payment |
| `build_defenses` | Build Defenses | public | 1 | structures | structures |
| `lost_outposts` | Discover Lost Outposts | public | 1 | attached_planets | territory |
| `engineer_marvel` | Engineer a Marvel | public | 1 | capital_ship_systems | fleet_shape |
| `deep_space` | Explore Deep Space | public | 1 | planetless_systems | fleet |
| `infrastructure` | Improve Infrastructure | public | 1 | structures_away | structures |
| `make_history` | Make History | public | 1 | notable_systems | fleet |
| `outer_rim` | Populate the Outer Rim | public | 1 | on_the_rim | fleet |
| `push_boundaries` | Push Boundaries | public | 1 | weaker_neighbours | territory |
| `raise_fleet` | Raise a Fleet | public | 1 | fleet_in_one_system | fleet_shape |
| `centralize_trade` | Centralize Galactic Trade | public | 2 | cost_trade_goods | payment |
| `conquer` | Conquer the Weak | public | 2 | rival_home_planets | territory |
| `brain_trust` | Form Galactic Brain Trust | public | 2 | tech_specialties | territory |
| `golden_age` | Found a Golden Age | public | 2 | cost_spend_resources | payment |
| `galvanize` | Galvanize the People | public | 2 | cost_tokens | payment |
| `manipulate_law` | Manipulate Galactic Law | public | 2 | cost_spend_influence | payment |
| `master_science` | Master the Sciences | public | 2 | colours_2 | technology |
| `revolutionize` | Revolutionize Warfare | public | 2 | unit_upgrades | technology |
| `subdue` | Subdue the Galaxy | public | 2 | non_home | territory |
| `unify_colonies` | Unify the Colonies | public | 2 | same_trait | territory |
| `supremacy` | Achieve Supremacy | public | 2 | capital_ships_rival_or_mecatol | fleet_shape |
| `become_legend` | Become a Legend | public | 2 | notable_systems | fleet |
| `command_armada` | Command an Armada | public | 2 | fleet_in_one_system | fleet_shape |
| `massive_cities` | Construct Massive Cities | public | 2 | structures | structures |
| `control_borderlands` | Control the Borderlands | public | 2 | on_the_rim | fleet |
| `vast_reserves` | Hold Vast Reserves | public | 2 | cost_all_three | payment |
| `vast_territories` | Patrol Vast Territories | public | 2 | planetless_systems | fleet |
| `protect_border` | Protect the Border | public | 2 | structures_away | structures |
| `ancient_monuments` | Reclaim Ancient Monuments | public | 2 | attached_planets | territory |
| `distant_lands` | Rule Distant Lands | public | 2 | distinct_rival_home_reaches | territory |
| `ans` | Adapt New Strategies | secret | 1 | faction_technologies | technology |
| `btgk` | Become the Gatekeeper | secret | 1 | wormhole_kinds | fleet |
| `ctr` | Control the Region | secret | 1 | ships_in_systems | fleet |
| `csl` | Cut Supply Lines | secret | 1 | rival_dock_systems_with_ships | fleet |
| `dtgs` | Destroy Their Greatest Ship | secret | 1 | (no progress) | combat |
| `eap` | Establish a Perimeter | secret | 1 | units_pds | structures |
| `faa` | Forge an Alliance | secret | 1 | planets_of_trait_cultural | territory |
| `fsn` | Form a Spy Network | secret | 1 | action_cards | holdings |
| `fwm` | Fuel the War Machine | secret | 1 | units_spacedock | structures |
| `gamf` | Gather a Mighty Fleet | secret | 1 | units_dreadnought | production |
| `lsc` | Learn the Secrets of the Cosmos | secret | 1 | ship_systems_beside_anomaly | fleet |
| `mew` | Make an Example of Their World | secret | 1 | (no progress) | abilities |
| `mlp` | Master the Laws of Physics | secret | 1 | same_colour_technologies | technology |
| `mrm` | Mine Rare Metals | secret | 1 | planets_of_trait_hazardous | territory |
| `mp` | Monopolize Production | secret | 1 | planets_of_trait_industrial | territory |
| `ose` | Occupy the Seat of the Empire | secret | 1 | mecatol_ships_while_controlling | fleet |
| `sar` | Spark a Rebellion | secret | 1 | (no progress) | combat |
| `te` | Threaten Enemies | secret | 1 | ship_systems_beside_rival_home | fleet |
| `ttfd` | Turn Their Fleets to Dust | secret | 1 | (no progress) | ttfd |
| `uf` | Unveil Flagship | secret | 1 | (no progress) | abilities |
| `bam` | Become a Martyr | secret | 1 | (no progress) | NONE: unsteerable (anti-goal: scored only by losing a home planet) |
| `baf` | Betray a Friend | secret | 1 | (no progress) | abilities |
| `btv` | Brave the Void | secret | 1 | (no progress) | combat |
| `dts` | Darken the Skies | secret | 1 | (no progress) | combat |
| `dfat` | Defy Space and Time | secret | 1 | units_in_nexus | fleet |
| `dyp` | Demonstrate Your Power | secret | 1 | (no progress) | combat |
| `dhw` | Destroy Heretical Works | secret | 1 | relic_fragments | holdings |
| `dp` | Dictate Policy | secret | 1 | laws | agenda |
| `dtd` | Drive the Debate | secret | 1 | (no progress) | agenda |
| `eh` | Establish Hegemony | secret | 1 | combined_value_influence | territory |
| `fwp` | Fight with Precision | secret | 1 | (no progress) | abilities |
| `fc` | Foster Cohesion | secret | 1 | neighbours | fleet |
| `hrm` | Hoard Raw Materials | secret | 1 | combined_value_resources | territory |
| `mtm` | Mechanize the Military | secret | 1 | mechs_on_distinct_planets | production |
| `otf` | Occupy the Fringe | secret | 1 | ground_forces_on_one_planet | fleet_shape |
| `pem` | Produce en Masse | secret | 1 | production_in_one_system | production |
| `pe` | Prove Endurance | secret | 1 | (no progress) | NONE: unsteerable (outlasting the table wastes command tokens; already scored 41% unsteered) |
| `sai` | Seize an Icon | secret | 1 | legendary_planets | territory |
| `syc` | Stake Your Claim | secret | 1 | shared_planet_systems | territory |
| `sb` | Strengthen Bonds | secret | 1 | rival_note_issuers | holdings |

## Final decisions (bot version 4)

All of B's planners ship on in bot version 4: `bw_payment`, `bw_technology`, `bw_combat` (with structures), and the second-round knobs
`bw_combat_abilities`, `bw_agenda`, `bw_holdings`, `bw_ttfd`, `bw_production`. Most of them measure as zero alone (see the tables above);
they ship because the combined 400-table runs are positive (v4 against v2 and v3, `docs/RULE_BOT_OPTIMISATION.md` Round 3) and they cost
little. The fleet-shape group (Agent A's `bw_fleet_shape`) is excluded: it lost directly against build alone.
