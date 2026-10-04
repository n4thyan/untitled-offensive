// mp_toccoa.gsc — per-map script for our custom training-ground map.
//
// This is ORIGINAL project content, authored for mp_toccoa. It deliberately
// mirrors the structure of a stock CoD1 map script (fog, ambient, faction and
// soldier model selection, layout image, retrieval-object text) so the retail
// gametype scripts drive the map through exactly the same path they drive
// mp_carentan. The only omissions are the per-map `_fx` include and the
// search-and-retrieve objective wiring, neither of which this map has.
//
// Everything it references resolves against a retail Call of Duty 1.1 install:
// the faction and soldier model names, the menu assets, the ambient alias and
// the localised objective strings below. No proprietary text is copied here --
// the strings are keyed to retail's localisation table, the map just names
// which keys it wants.

main()
{
	// Open sky, matching the map's out-of-doors look. The far plane is pulled
	// in from the stock 16500 because the arena is only ~4000 units across and
	// a longer draw distance buys nothing but overdraw.
	setCullFog (0, 12000, 0.7, 0.85, 1.0, 0);

	// Camp Toccoa is a live-fire range: wind, birds, distant range activity.
	// This alias exists in the retail sound table.
	ambientPlay("ambient_mp_carentan");

	// The stock loader: sets up the gametype hooks every map relies on.
	maps\mp\_load::main();

	// Factions. American vs German, as on the western-front maps this
	// training-camp theme belongs to.
	game["allies"] = "american";
	game["axis"] = "german";

	// Soldier models. Airborne for the allies and fallschirmjager-grey for the
	// axis: standard western-front pairing, and both exist in retail.
	game["american_soldiertype"] = "airborne";
	game["american_soldiervariation"] = "normal";
	game["german_soldiertype"] = "fallschirmjagergrey";
	game["german_soldiervariation"] = "normal";

	game["attackers"] = "allies";
	game["defenders"] = "axis";

	// Loading-screen radar image, resolved from ui_mp assets. Absent for this
	// map, which simply means the stock layout falls back to no image; there
	// is deliberately no replacement art committed to this repository.
	game["layoutimage"] = "mp_carentan";

	// Retrieval-mode object text. Search & Retrieve is not wired up on this
	// map yet, but the keys are declared so the gametype script finds them and
	// does not print an undefined-object message.
	level.obj["Code Book"] = (&"RE_OBJ_CODE_BOOK");
	level.obj["Field Radio"] = (&"RE_OBJ_FIELD_RADIO");
	game["re_attackers"] = "allies";
	game["re_defenders"] = "axis";
	game["re_attackers_obj_text"] = (&"RE_OBJ_CARENTAN_OBJ_ATTACKER");
	game["re_defenders_obj_text"] = (&"RE_OBJ_CARENTAN_OBJ_DEFENDER");
	game["re_spectator_obj_text"] = (&"RE_OBJ_CARENTAN_SPECTATOR");
	game["re_attackers_intro_text"] = (&"RE_OBJ_CARENTAN_SPAWN_ATTACKER");
	game["re_defenders_intro_text"] = (&"RE_OBJ_CARENTAN_SPAWN_DEFENDER");
}