use std::path::PathBuf;

use abi_stable::std_types::{ROption, RVec};
use anyrun_interface::{Match, PluginRef};
use gtk::{glib, pango, prelude::*};
use gtk4 as gtk;
use relm4::prelude::*;

use crate::style_names;

pub struct PluginMatch {
    pub content: Match,
    pub row: gtk::ListBoxRow,
}

#[relm4::factory(pub)]
impl FactoryComponent for PluginMatch {
    type Init = Match;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;
    view! {
        gtk::ListBoxRow {
            set_widget_name: style_names::MATCH,
            set_height_request: 32,
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                set_widget_name: style_names::MATCH,
                set_hexpand: true,

                #[name = "icon"]
                gtk::Image {
                    set_pixel_size: 32,
                    set_widget_name: style_names::MATCH,
                },

                #[name = "text"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_widget_name: style_names::MATCH,
                    set_hexpand: true,
                    set_vexpand: true,

                    gtk::Label {
                        set_widget_name: style_names::MATCH_TITLE,
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Center,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_natural_wrap_mode: gtk::NaturalWrapMode::Word,
                        set_wrap_mode: pango::WrapMode::WordChar,
                        set_use_markup: self.content.use_pango,
                        set_label: &self.content.title,
                    },

                    #[name = "description"]
                    gtk::Label {
                        set_widget_name: style_names::MATCH_DESC,
                        set_wrap: true,
                        set_xalign: 0.0,
                        set_use_markup: self.content.use_pango,
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Center,
                    }
                }
            }
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();

        self.row = root;

        match &self.content.icon {
            ROption::RSome(icon) => {
                let path = PathBuf::from(icon.to_string());
                if path.is_absolute() {
                    widgets.icon.set_from_file(Some(path));
                } else {
                    widgets.icon.set_icon_name(Some(icon));
                }
            }
            ROption::RNone => widgets.icon.set_visible(false),
        }

        match &self.content.description {
            ROption::RSome(desc) => widgets.description.set_label(desc),
            ROption::RNone => widgets.description.set_visible(false),
        }

        widgets
    }

    fn init_model(content: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        let row = gtk::ListBoxRow::default();

        Self { row, content }
    }
}

pub struct PluginBox {
    pub plugin: PluginRef,
    visible: bool,
    enabled: bool,
    pub matches: FactoryVecDeque<PluginMatch>,
}

#[derive(Debug, Clone)]
pub enum PluginBoxInput {
    EntryChanged(String),
    Enable(bool),
}

#[derive(Debug)]
pub enum PluginBoxOutput {
    MatchesLoaded,
}

#[relm4::factory(pub)]
impl FactoryComponent for PluginBox {
    type Init = PluginRef;
    type Input = PluginBoxInput;
    type Output = PluginBoxOutput;
    type CommandOutput = RVec<Match>;
    type ParentWidget = gtk::Box;

    view! {
        gtk::Box {
            #[watch]
            set_visible: self.visible,

            #[local_ref]
            matches -> gtk::ListBox {}
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let matches = self.matches.widget();

        let widgets = view_output!();

        widgets
    }

    fn init_model(plugin: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        let matches = FactoryVecDeque::builder()
            .launch(
                gtk::ListBox::builder()
                    .css_name(style_names::PLUGIN)
                    .hexpand(true)
                    .build(),
            )
            .detach();

        Self {
            plugin,
            visible: false,
            enabled: true,
            matches,
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: FactorySender<Self>,
    ) {
        match message {
            PluginBoxInput::EntryChanged(input) => {
                if self.enabled {
                    sender.spawn_command(glib::clone!(
                        #[strong(rename_to = plugin)]
                        self.plugin,
                        move |sender| {
                            sender.emit(plugin.get_matches()(input.into()));
                        }
                    ));
                }
            }
            PluginBoxInput::Enable(enable) => {
                self.enabled = enable;
                self.visible = enable;

                if !enable {
                    self.matches.guard().clear();
                }
            }
        }

        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        matches: Self::CommandOutput,
        sender: FactorySender<Self>,
    ) {
        if !self.enabled {
            return;
        }
        self.visible = !matches.is_empty();
        {
            let mut guard = self.matches.guard();

            guard.clear();

            for _match in matches {
                guard.push_back(_match);
            }
        }

        sender.output(PluginBoxOutput::MatchesLoaded).unwrap();

        self.update_view(widgets, sender);
    }
}
