// mp_rust.gsc — original UNTITLED bridge script for the imported MW2 map.
// The map contributes world data only; stock CoD1 multiplayer scripts provide
// teams, classes, weapons, scoring, death, killcam and respawn.

main()
{
	setCullFog (0, 16000, 0.72, 0.78, 0.84, 0);
	ambientPlay("ambient_mp_carentan");

	maps\mp\_load::main();

	game["allies"] = "american";
	game["axis"] = "german";
	game["american_soldiertype"] = "airborne";
	game["american_soldiervariation"] = "normal";
	game["german_soldiertype"] = "fallschirmjagergrey";
	game["german_soldiervariation"] = "normal";
	game["attackers"] = "allies";
	game["defenders"] = "axis";

	// Reuse an installed CoD1 loading image; no MW2 artwork is distributed.
	game["layoutimage"] = "mp_carentan";
}
