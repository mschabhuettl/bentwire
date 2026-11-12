# bentwire-core rule coverage

One line per token of `grep -o 'R-[A-Z0-9-]*\|B-[A-Z0-9-]*' docs/spec.md | sort -u`, in
that order: `ID | tests` (test names in `crates/bentwire-core/src`, run with
`cargo test -p bentwire-core`), or the reason a rule is not testable in the core and where
it is tested. Regenerate the ID column with the grep above and diff it against
`cut -d' ' -f1 crates/bentwire-core/RULES.md | grep -v '^#' | grep .`.

B- | prefix token (from "B-<AREA>-n" in spec 0), not a rule
B-ANIM-1 | b_sw_a_r_anim_7_b_anim_1_loop_stopped_resumes_through_exit, r_anim_7_b_anim_1_stop_cuts_and_no_animation_requests_keep_previous
B-ANIM-2 | b_anim_2_r_anim_3_completion_waits_for_sound
B-ASK- | wildcard reference to the B-ASK rules, see each B-ASK-n
B-ASK-1 | b_ask_1_contract || other crates: bentwired: b_ask_1_parse_types
B-ASK-2 | b_ask_2_keyboard_and_clicks, b_ask_2_sets_and_results || other crates: bentwire: b_l10n_2_b_ask_2_result_names
B-ASK-3 | b_ask_3_r_ob_16_markup
B-ASK-4 | b_ask_4_b_wl_8_phases_and_answer, b_ask_4_hidden_and_too_big
B-ASK-5 | b_ask_5_validation
B-ASK-6 | b_ask_6_autodown
B-ASK-7 | b_ask_7_matrix_modal, b_ask_7_matrix_autodown_and_switch, b_ask_7_autodown_hide_closes_with_status_3, b_ask_7_ask_shown_during_drag_is_placed_at_drag_end || other crates: bentwired: b_ask_7_autodown_speak_and_second_ask_dismiss, b_ask_7_b_char_5_character_switch_closes_ask, b_ask_7_drag_hides_and_replaces_modal_balloon, b_ask_7_hide_stop_and_stopall_close_the_ask ...
B-BAL-1 | b_bal_1_no_balloon
B-BAL-2 | b_bal_2_font_choice (family, config override, px = |font_height|); TM metrics from the font in bentwire-render || other crates: bentwire-render: b_bal_2_draw_text_marks_pixels_inside_cell, b_bal_2_metrics_from_font, b_bal_2_text_width_is_sum_of_advances
B-BAL-3 | b_bal_3_anchor_is_lowest_mouth_point, b_bal_3_r_bal_4_r_bal_5_vectors
B-BAL-4 | b_bal_4_mouth_point_from_bbox
B-CFG-1 | b_cfg_1_defaults (struct and defaults); file parsing and SIGHUP in bentwired || other crates: bentwired: b_cfg_1_defaults_and_parse, b_cfg_1_sighup_reloads_language
B-CFG-2 | b_cfg_2_b_run_1_persist_throttle_and_quit, b_char_4_b_cfg_2_unchosen_state_keeps_position_only, b_char_4_b_cfg_2_chosen_by_select_and_gallery_ok || other crates: bentwired: b_cfg_2_state_round_trip, b_cfg_2_b_char_4_chosen_upgrade_from_0_1_1
B-CFG-3 | b_cfg_3_first_run_and_b_char_4
B-CHAR-1 | b_char_1_rescan_on_use_and_b_char_4_default_when_not_chosen (rescans through the loader); discovery itself is file I/O, owner: bentwired (discovery) || other crates: bentwired: b_char_1_a_pack_1_discovery, b_char_1_new_characters_without_restart_and_select_hints
B-CHAR-2 | b_fb_1_b_char_2_play_errors_and_unavailable (unavailable animation fails AnimationUnavailable), r_state_1_uniform_member_and_r_state_2 (unavailable members skipped); decoding failures detected by the loader in bentwire-character/bentwired
B-CHAR-3 | b_char_3_display_names
B-CHAR-4 | b_cfg_3_first_run_and_b_char_4, b_char_4_state_then_config_then_sorted, b_char_4_startup_order, b_char_4_b_cfg_2_unchosen_state_keeps_position_only, b_char_4_b_cfg_2_chosen_by_select_and_gallery_ok, b_char_1_rescan_on_use_and_b_char_4_default_when_not_chosen, b_char_4_rescan_installs_a_character_when_none_loaded || other crates: bentwired: spec_5_5_characters_install_classics
B-CHAR-5 | b_char_5_switch, b_char_5_invalid_character, b_char_5_new_character_keeps_the_bottom_centre, b_char_5_foot_and_stand_at || other crates: bentwired: b_ask_7_b_char_5_character_switch_closes_ask, b_char_5_rest_box_of_the_placeholder, library_gallery_walk_keeps_the_bottom_centre (real data)
B-CHAR-6 | b_char_6_install_hint_is_an_internal_speak; the start condition and the flag are the daemon's || other crates: bentwired: b_char_6_install_hint_once_with_only_builtin
B-CLI-1 | not testable in core: the CLI (crates/bentwire) || other crates: bentwired: spec_5_5_characters_install_classics
B-CLI-2 | not testable in core: the CLI (crates/bentwire) || other crates: bentwire: spec_5_5_select_hints; bentwired: b_char_1_new_characters_without_restart_and_select_hints
B-CORE-1 | structural: the crate has no I/O; every engine test runs it with fake clock and draws
B-CORE-2 | b_core_2_draw_sources
B-CORE-3 | structural: all timers come from Core::next_deadline/tick on the caller's clock; every engine test uses the fake clock (core_tests::T::advance)
B-FAM-1 | structural: family dispatch in player.rs; per-row tests: b_seq_a_*, b_o97_*, b_sw_a_*, b_sw_o_*, b_idle_t_*, b_idle_o_office97, b_fb_1_b_o97_5_*
B-FB-1 | b_fb_1_b_char_2_play_errors_and_unavailable, b_fb_1_b_o97_5_alias_and_office97_groups, b_fb_1_error_lists_chain
B-FB-2 | b_fb_2_rover_like, b_fb_2_courtney_earl_merlin_qmark_like, b_fb_2_office_names_resolve_to_themselves || other crates: bentwired: b_fb_2_table_on_converted_acs
B-GAL-1 | b_gal_1_gallery, b_gal_1_hide_closes_as_cancel, b_gal_1_running_ask_is_interrupted_when_gallery_opens, b_gal_1_balloon_stays_while_it_fits_next_to_the_new_character || other crates: bentwired: b_gal_1_gallery_next_cancel_and_ok, library_gallery_walk_keeps_the_bottom_centre (real data)
B-IDLE-O | b_idle_o_office97
B-IDLE-T | b_idle_t_r_idle_1_r_idle_2_r_idle_3_vector, b_idle_t_r_idle_4_play_interrupts_idle
B-L10N-1 | b_l10n_1_language
B-L10N-2 | b_l10n_2_captions, b_ask_2_sets_and_results; CLI result names used by crates/bentwire || other crates: bentwire: b_l10n_2_b_ask_2_result_names
B-LIP-1 | b_lip_1_r_lip_2_quantisation
B-LIP-2 | b_lip_2_no_mouth_during_text_speak
B-MENU-1 | b_menu_1_r_ui_3_b_menu_2_b_menu_3
B-MENU-2 | b_menu_1_r_ui_3_b_menu_2_b_menu_3, b_menu_2_strings || other crates: bentwire-render: b_menu_2_menu_en_de_golden, b_menu_2_mnemonic_parsing
B-MENU-3 | b_menu_1_r_ui_3_b_menu_2_b_menu_3 (Output::OpenConfig); writing the file and xdg-open in bentwired || other crates: bentwired: b_menu_3_options_writes_config_and_opens_it
B-MENU-4 | b_menu_4_animate_shuffle
B-MOVE-1 | b_move_1_slide_vectors, b_move_1_period_floor
B-O97-1 | b_o97_1_branches
B-O97-2 | b_o97_2_zero_frames_and_blank
B-O97-3 | b_o97_3_coalescing_vector, b_o97_3_minimum_tick_and_late_reset
B-O97-4 | b_o97_4_play_group_picks_variant
B-O97-5 | b_fb_1_b_o97_5_alias_and_office97_groups
B-OB- | wildcard reference to the B-OB rules, see each B-OB-n
B-OB-1 | r_ob_32_r_ob_11_b_ob_1_r_ob_4_button_row_and_fonts || other crates: bentwire-render: b_ob_1_core_layout_heading_markup_golden, b_ob_1_core_layout_labels_checkboxes_golden, b_ob_1_core_layout_numbers_custom_buttons_golden, b_ob_1_office_bullets_numbers_golden ...
B-OB-2 | b_ob_2_r_ob_27_tail
B-OB-3 | b_ob_3_b_ob_4_r_ob_17_r_ob_20_r_ob_34_placement_and_fit
B-OB-4 | b_ob_4_moves_once_not_on_refresh, b_ob_3_b_ob_4_r_ob_17_r_ob_20_r_ob_34_placement_and_fit
B-PICK-A | b_pick_a_speakable_keeps_animation_with_mouths
B-PIX-1 | not testable in core: compositing; owner: bentwire-character, bentwire-render || other crates: bentwire-render: b_pix_1_blend_partial_alpha_matches_5_3
B-PIX-2 | not testable in core: mouth clipping is a pixel step; owner: bentwire-render (core never sets a mouth in v1, b_lip_2_no_mouth_during_text_speak) || other crates: bentwire-render: b_pix_2_mouth_clipped_to_base
B-PIX-3 | not testable in core: alpha shape from pixels; owner: bentwire-render || other crates: bentwire-render: b_pix_3_input_region_is_alpha_shape
B-Q-1 | r_q_1_b_q_1_ids_and_reply_before_signals || other crates: bentwired: b_q_1_reply_before_signal_and_b_q_2_unicast
B-Q-2 | b_q_2_r_q_5_unicast_and_events || other crates: bentwired: b_q_1_reply_before_signal_and_b_q_2_unicast, b_q_2_removed_before_start_gets_only_complete_status_3
B-Q-3 | b_q_3_hidden_moveto_r_move_4
B-Q-4 | r_q_4_b_q_4_stop_zero
B-Q-5 | b_q_5_stop_all || other crates: bentwired: b_q_5_stopall_unknown_type_is_invalid_args
B-REN-1 | not core: render decision, owner: bentwire-render
B-REN-2 | not core: render decision, owner: bentwire-render
B-REN-3 | not core: render decision, owner: bentwire-render
B-REN-4 | not core: render decision, owner: bentwire-render || other crates: bentwire-render: b_ren_4_region_follows_displayed_pixels
B-REN-5 | not core: render decision, owner: bentwire-render, bentwired
B-REN-6 | not core: render decision, owner: bentwire-render
B-REN-7 | not core: render decision, owner: bentwire-render
B-REN-8 | office::fonts() sets Tahoma 13/16 with FontSpec.cell = true (r_ob_32_r_ob_11_b_ob_1_r_ob_4_button_row_and_fonts); cell-height metrics in bentwire-render
B-RUN-1 | b_cfg_2_b_run_1_persist_throttle_and_quit (Quit: reply, state, quit); bus name and signals in bentwired || other crates: bentwired: b_run_1_dbus_activation_starts_the_daemon, b_run_1_quit_and_sigterm_save_state_and_exit_0, b_run_1_second_instance_exits_already_running
B-SCALE-1 | b_scale_1_set_keeps_the_bottom_centre_and_persists, b_scale_1_pulled_inside_the_output, b_scale_1_precedence_state_then_config, b_scale_1_sighup_applies_until_the_user_chooses, b_scale_1_geometry_at_3x_equals_a_3x_character, b_scale_1_drag_at_3x_and_change_during_a_drag, b_scale_1_switch_and_output_change_use_the_factor, b_scale_1_size_submenu_pointer_keys_and_tray, b_scale_1_size_menu_german, b_scale_1_foot_size_and_rest_box_follow_the_factor; pictures, files, D-Bus and CLI outside the core || other crates: bentwire-render: b_scale_1_size_submenu_golden; bentwired: b_scale_1_bitmap_character_pixels_replicated_and_region_doubled, b_scale_1_vector_character_drawn_at_the_target_density, b_scale_1_config_and_state_keys, b_scale_1_tray_size_submenu_is_a_radio_group, b_scale_1_test1_snapshot_and_pixels_at_1x_and_2x, b_scale_1_drag_and_balloon_at_3x, b_scale_1_cli_dbus_round_trip_and_persistence, b_scale_1_size_submenu_through_test1_and_trace
B-SEQ-A | b_seq_a_r_anim_4_branch_vector
B-SHOW-3 | b_show_3_fast_show_picture
B-SHOW-4 | b_show_4_cause_codes
B-SPEAK- | wildcard reference, see B-SPEAK-1 and B-SPEAK-2
B-SPEAK-1 | b_speak_1_tags, b_speak_1_r_q_7_bookmark_range
B-SPEAK-2 | r_bal_8_r_bal_7_b_speak_2_r_speak_4_pacing_vector
B-SW-A | b_sw_a_r_anim_7_b_anim_1_loop_stopped_resumes_through_exit, b_sw_a_return_after_finished, b_sw_a_r_anim_6_transition_exit_and_showing_drops, b_sw_a_agent15_exit_end_plays_return
B-SW-O | b_sw_o_exit_and_transitions, b_sw_o_start_pose
B-TRAY-1 | b_tray_1_r_ui_4_activate, b_tray_1_title_cut, b_ui_4_user_hide (tray menu); SNI in bentwired || other crates: bentwired: b_tray_1_labels_and_icon
B-UI-1 | r_ui_1_b_ui_1_click_and_double_click || other crates: bentwired: b_ui_1_click_and_r_ui_2_drag_signals
B-UI-4 | b_ui_4_user_hide, b_ui_4_user_hide_in_gallery_reports_hidden_by_user || other crates: bentwired: b_ui_4_user_hide_from_menu_interrupts_with_hidden_by_user
B-WL-1 | output rects in global logical coords are core input (OutputInfo); used by b_wl_2_output_choice, b_ob_3_*; Wayland side in bentwired
B-WL-2 | b_wl_2_b_wl_4_extreme_positions_do_not_overflow, b_wl_2_output_choice
B-WL-3 | not testable in core: layer choice; bentwired / T-6 K2
B-WL-4 | b_wl_4_output_change_pulls_back, b_wl_2_b_wl_4_extreme_positions_do_not_overflow, b_wl_4_pull_inside || other crates: bentwired: b_wl_4_output_removal_pulls_the_frame_back
B-WL-5 | structural: pointer events carry global coordinates (api::PointerEvent); conversion in bentwired
B-WL-6 | not testable in core: buffers, scale, input region; bentwired and bentwire-render || other crates: bentwire-render: b_wl_6_argb8888_premultiplied_byte_order, b_wl_6_hidpi_picture_used_as_is_region_logical, b_wl_6_integer_upscale_and_byte_order; bentwired: b_wl_6_scale_of_rotated_and_fractional_outputs
B-WL-7 | core part: 1:1 drag from the grab offset in r_ui_2_r_win_4_drag; margin compensation in bentwired
B-WL-8 | b_ask_4_b_wl_8_phases_and_answer || other crates: bentwire-render: b_wl_8_region_rects_merge_rows
B-WL-9 | core part: menu model and keys in b_menu_1_r_ui_3_b_menu_2_b_menu_3; xdg_popup in bentwired || other crates: bentwire-render: b_wl_9_menu_hit_skips_separator
R- | prefix token (from "R-..." in spec 0), not a rule
R-1 | grep artefact (suffix of B-CHAR-1/A-CHAR-1/R-DIR-1), not a rule
R-2 | grep artefact (suffix of A-CHAR-2), not a rule
R-3 | grep artefact (suffix of A-CHAR-3), not a rule
R-4 | grep artefact (suffix of A-CHAR-4), not a rule
R-ANIM-1 | covered by r_anim_2_frame_clock (frame ms used unchanged); the x10 conversion is the converter's (owner: bentwire-convert)
R-ANIM-10 | structural: one Player per loaded character (core::Char.player); every player test exercises it
R-ANIM-2 | r_anim_2_frame_clock, r_anim_2_late_frame_no_catch_up
R-ANIM-3 | b_anim_2_r_anim_3_completion_waits_for_sound
R-ANIM-4 | b_seq_a_r_anim_4_branch_vector || other crates: bentwire-character: r_anim_4_cumulative_branch_walk
R-ANIM-5 | r_anim_5_invisible_frames, r_anim_5_zero_ms_frame_with_layers_is_shown
R-ANIM-6 | b_sw_a_r_anim_6_transition_exit_and_showing_drops
R-ANIM-7 | b_sw_a_r_anim_7_b_anim_1_loop_stopped_resumes_through_exit, r_anim_7_b_anim_1_stop_cuts_and_no_animation_requests_keep_previous
R-ANIM-8 | r_anim_8_sound_gating, r_anim_8_sound_commands || other crates: bentwired: r_anim_8_sound_failed_without_device
R-ANIM-9 | r_anim_9_hidden_play_fails
R-BAL- | wildcard reference, see each R-BAL-n
R-BAL-1 | r_bal_1_r_bal_3_size || other crates: bentwired: r_bal_1_balloon_defaults_and_colours
R-BAL-10 | r_bal_10_no_auto_hide_and_click
R-BAL-2 | r_bal_2_defaults
R-BAL-3 | r_bal_1_r_bal_3_size
R-BAL-4 | b_bal_3_r_bal_4_r_bal_5_vectors (tail 16/4 geometry); drawing in bentwire-render || other crates: bentwire-render: r_bal_4_agent_balloon_golden, r_bal_4_round_rect_corners_cut
R-BAL-5 | b_bal_3_r_bal_4_r_bal_5_vectors, r_bal_5_clamps, r_bal_5_tiny_balloon_does_not_panic
R-BAL-6 | r_bal_6_word_wrap
R-BAL-7 | r_bal_8_r_bal_7_b_speak_2_r_speak_4_pacing_vector, r_bal_7_not_shown_during_drag
R-BAL-8 | r_bal_8_r_bal_7_b_speak_2_r_speak_4_pacing_vector, r_bal_8_r_ui_5_interval
R-BAL-9 | r_bal_9_scroll || other crates: bentwire-render: r_bal_9_agent_balloon_scrolling_golden
R-DIR-1 | r_dir_1_vectors, r_dir_1_degenerate
R-GEST-1 | r_gest_1_via_core, r_gest_1_vectors
R-IDLE | wildcard reference ("R-IDLE applies"), see R-IDLE-1..5
R-IDLE-1 | b_idle_t_r_idle_1_r_idle_2_r_idle_3_vector, r_idle_4_idle_on_false_and_r_idle_1_conditions
R-IDLE-2 | b_idle_t_r_idle_1_r_idle_2_r_idle_3_vector
R-IDLE-3 | b_idle_t_r_idle_1_r_idle_2_r_idle_3_vector
R-IDLE-4 | b_idle_t_r_idle_4_play_interrupts_idle, r_idle_4_idle_on_false_and_r_idle_1_conditions
R-IDLE-5 | r_idle_5_missing_level
R-LIP-1 | r_lip_1_table_size_and_unique, r_lip_1_vectors
R-LIP-2 | b_lip_1_r_lip_2_quantisation
R-LIP-3 | replaced by B-LIP-2: b_lip_2_no_mouth_during_text_speak
R-MOVE- | wildcard reference, see R-MOVE-1..4
R-MOVE-1 | r_move_1_r_move_2_r_move_3_phases, r_move_1_vectors
R-MOVE-2 | r_move_1_r_move_2_r_move_3_phases, r_move_2_drag_cancels_slide, r_move_2_drag_cancel_keeps_output_and_emits_no_target_move
R-MOVE-3 | r_move_1_r_move_2_r_move_3_phases
R-MOVE-4 | b_q_3_hidden_moveto_r_move_4
R-OB- | wildcard reference, see each R-OB-n
R-OB-1 | background colour is drawn by bentwire-render (B-OB-1); not a core decision
R-OB-11 | r_ob_32_r_ob_11_b_ob_1_r_ob_4_button_row_and_fonts || other crates: bentwire-render: r_ob_11_office_button_states_golden
R-OB-16 | b_ask_3_r_ob_16_markup
R-OB-17 | b_ob_3_b_ob_4_r_ob_17_r_ob_20_r_ob_34_placement_and_fit
R-OB-2 | r_ob_2_text_colours
R-OB-20 | b_ob_3_b_ob_4_r_ob_17_r_ob_20_r_ob_34_placement_and_fit
R-OB-27 | b_ob_2_r_ob_27_tail
R-OB-31 | r_ob_31_sizes_landscape_portrait
R-OB-32 | r_ob_32_r_ob_11_b_ob_1_r_ob_4_button_row_and_fonts || other crates: bentwire-render: r_ob_32_office_button_sets_golden
R-OB-34 | b_ob_3_b_ob_4_r_ob_17_r_ob_20_r_ob_34_placement_and_fit
R-OB-4 | r_ob_32_r_ob_11_b_ob_1_r_ob_4_button_row_and_fonts
R-OB-5 | r_ob_35_r_ob_5_vertical_spacing || other crates: bentwire-render: r_ob_5_icons_golden
R-OB-7 | r_ob_35_r_ob_5_vertical_spacing (prefix column floor 17, label text at column + 5, bullet prefix)
R-OB-8 | not core: label bullet art (cited by B-REN-7); owner: bentwire-render
R-Q- | wildcard reference, see each R-Q-n
R-Q-1 | r_q_1_b_q_1_ids_and_reply_before_signals
R-Q-3 | r_q_3_immediate_calls
R-Q-4 | r_q_4_b_q_4_stop_zero
R-Q-5 | b_q_2_r_q_5_unicast_and_events
R-Q-6 | r_q_6_loop_blocks_until_stop, r_q_6_loop_without_exit_stop_then_next_request || other crates: bentwired: r_q_6_stop_lets_next_request_run_after_loop
R-Q-7 | b_speak_1_r_q_7_bookmark_range
R-SHOW-1 | r_show_1_and_r_show_2
R-SHOW-2 | r_show_1_and_r_show_2
R-SHOW-3 | replaced by B-SHOW-3: b_show_3_fast_show_picture
R-SPEAK- | wildcard reference, see R-SPEAK-1..4
R-SPEAK-1 | r_speak_1_speakable_then_output_and_bookmarks
R-SPEAK-2 | r_speak_2_empty_and_hidden
R-SPEAK-3 | r_speak_3_alternative_draw, r_speak_3_alternatives
R-SPEAK-4 | b_speak_2: r_bal_8_r_bal_7_b_speak_2_r_speak_4_pacing_vector (Speak completes with status 0)
R-STATE-1 | r_state_1_uniform_member_and_r_state_2
R-STATE-2 | r_state_1_uniform_member_and_r_state_2, r_state_2_names_ignore_case
R-UI-1 | r_ui_1_b_ui_1_click_and_double_click
R-UI-2 | r_ui_2_r_win_4_drag, r_ui_2_hide_during_drag_forgets_press || other crates: bentwired: b_ui_1_click_and_r_ui_2_drag_signals
R-UI-3 | b_menu_1_r_ui_3_b_menu_2_b_menu_3
R-UI-4 | b_tray_1_r_ui_4_activate
R-UI-5 | r_bal_8_r_ui_5_interval (speaking_speed factor in the pacing formula)
R-WIN-1 | not testable in core: layer-shell surface; owner: bentwired
R-WIN-2 | not testable in core: alpha shape from pixels; owner: bentwire-render
R-WIN-4 | r_ui_2_r_win_4_drag
R-WIN-5 | not testable in core: focus is a compositor request; scene keyboard_on_demand in b_ask_4_b_wl_8_phases_and_answer; rest in bentwired
