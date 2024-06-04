/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { Action, IAction, Separator } from 'vs/base/common/actions';
import { Emitter } from 'vs/base/common/event';
import { ThemeIcon } from 'vs/base/common/themables';
import { localize } from 'vs/nls';
import { IInstantiationService } from 'vs/platform/instantiation/common/instantiation';
import { ITelemetryService } from 'vs/platform/telemetry/common/telemetry';
import { DidChangeProfilesEvent, isUserDataProfile, IUserDataProfile, IUserDataProfilesService, ProfileResourceType, ProfileResourceTypeFlags, toUserDataProfile, UseDefaultProfileFlags } from 'vs/platform/userDataProfile/common/userDataProfile';
import { IProfileResourceChildTreeItem, IUserDataProfileImportExportService, IUserDataProfileManagementService, IUserDataProfileService, IUserDataProfileTemplate } from 'vs/workbench/services/userDataProfile/common/userDataProfile';
import { Disposable, DisposableStore, toDisposable } from 'vs/base/common/lifecycle';
import { URI } from 'vs/base/common/uri';
import { equals } from 'vs/base/common/objects';
import { EditorModel } from 'vs/workbench/common/editor/editorModel';
import { ExtensionsResourceExportTreeItem, ExtensionsResourceImportTreeItem } from 'vs/workbench/services/userDataProfile/browser/extensionsResource';
import { SettingsResource, SettingsResourceTreeItem } from 'vs/workbench/services/userDataProfile/browser/settingsResource';
import { KeybindingsResource, KeybindingsResourceTreeItem } from 'vs/workbench/services/userDataProfile/browser/keybindingsResource';
import { TasksResource, TasksResourceTreeItem } from 'vs/workbench/services/userDataProfile/browser/tasksResource';
import { SnippetsResource, SnippetsResourceTreeItem } from 'vs/workbench/services/userDataProfile/browser/snippetsResource';
import { Codicon } from 'vs/base/common/codicons';
import { IDialogService } from 'vs/platform/dialogs/common/dialogs';
import { InMemoryFileSystemProvider } from 'vs/platform/files/common/inMemoryFilesystemProvider';
import { IFileService } from 'vs/platform/files/common/files';
import { generateUuid } from 'vs/base/common/uuid';
import { RunOnceScheduler } from 'vs/base/common/async';
import { IHostService } from 'vs/workbench/services/host/browser/host';
import { CancellationToken, CancellationTokenSource } from 'vs/base/common/cancellation';
import { ITreeItemCheckboxState } from 'vs/workbench/common/views';
import { API_OPEN_EDITOR_COMMAND_ID } from 'vs/workbench/browser/parts/editor/editorCommands';
import { SIDE_GROUP } from 'vs/workbench/services/editor/common/editorService';
import { ICommandService } from 'vs/platform/commands/common/commands';
import { IConfigurationService } from 'vs/platform/configuration/common/configuration';
import { CONFIG_NEW_WINDOW_PROFILE } from 'vs/workbench/common/configuration';

export type ChangeEvent = {
	readonly name?: boolean;
	readonly icon?: boolean;
	readonly flags?: boolean;
	readonly active?: boolean;
	readonly message?: boolean;
	readonly copyFrom?: boolean;
	readonly copyFlags?: boolean;
	readonly preview?: boolean;
	readonly disabled?: boolean;
	readonly newWindowProfile?: boolean;
};

export interface IProfileChildElement {
	readonly handle: string;
	readonly action?: IAction;
	readonly checkbox?: ITreeItemCheckboxState;
}

export interface IProfileResourceTypeElement extends IProfileChildElement {
	readonly resourceType: ProfileResourceType;
}

export interface IProfileResourceTypeChildElement extends IProfileChildElement {
	readonly label: string;
	readonly resource?: URI;
	readonly icon?: ThemeIcon;
}

export function isProfileResourceTypeElement(element: IProfileChildElement): element is IProfileResourceTypeElement {
	return (element as IProfileResourceTypeElement).resourceType !== undefined;
}

export function isProfileResourceChildElement(element: IProfileChildElement): element is IProfileResourceTypeChildElement {
	return (element as IProfileResourceTypeChildElement).label !== undefined;
}

export abstract class AbstractUserDataProfileElement extends Disposable {

	protected readonly _onDidChange = this._register(new Emitter<ChangeEvent>());
	readonly onDidChange = this._onDidChange.event;

	private readonly saveScheduler = this._register(new RunOnceScheduler(() => this.doSave(), 500));

	constructor(
		name: string,
		icon: string | undefined,
		flags: UseDefaultProfileFlags | undefined,
		isActive: boolean,
		@IUserDataProfileManagementService protected readonly userDataProfileManagementService: IUserDataProfileManagementService,
		@IUserDataProfilesService protected readonly userDataProfilesService: IUserDataProfilesService,
		@ICommandService protected readonly commandService: ICommandService,
		@IInstantiationService protected readonly instantiationService: IInstantiationService,
	) {
		super();
		this._name = name;
		this._icon = icon;
		this._flags = flags;
		this._active = isActive;
		this._register(this.onDidChange(e => {
			if (!e.message) {
				this.validate();
			}
			this.save();
		}));
	}

	private _name = '';
	get name(): string { return this._name; }
	set name(label: string) {
		if (this._name !== label) {
			this._name = label;
			this._onDidChange.fire({ name: true });
		}
	}

	private _icon: string | undefined;
	get icon(): string | undefined { return this._icon; }
	set icon(icon: string | undefined) {
		if (this._icon !== icon) {
			this._icon = icon;
			this._onDidChange.fire({ icon: true });
		}
	}

	private _flags: UseDefaultProfileFlags | undefined;
	get flags(): UseDefaultProfileFlags | undefined { return this._flags; }
	set flags(flags: UseDefaultProfileFlags | undefined) {
		if (!equals(this._flags, flags)) {
			this._flags = flags;
			this._onDidChange.fire({ flags: true });
		}
	}

	private _active: boolean = false;
	get active(): boolean { return this._active; }
	set active(active: boolean) {
		if (this._active !== active) {
			this._active = active;
			this._onDidChange.fire({ active: true });
		}
	}

	private _message: string | undefined;
	get message(): string | undefined { return this._message; }
	set message(message: string | undefined) {
		if (this._message !== message) {
			this._message = message;
			this._onDidChange.fire({ message: true });
		}
	}

	private _disabled: boolean = false;
	get disabled(): boolean { return this._disabled; }
	set disabled(saving: boolean) {
		if (this._disabled !== saving) {
			this._disabled = saving;
			this._onDidChange.fire({ disabled: true });
		}
	}

	getFlag(key: ProfileResourceType): boolean {
		return this.flags?.[key] ?? false;
	}

	setFlag(key: ProfileResourceType, value: boolean): void {
		const flags = this.flags ? { ...this.flags } : {};
		if (value) {
			flags[key] = true;
		} else {
			delete flags[key];
		}
		this.flags = flags;
	}

	validate(): void {
		if (!this.name) {
			this.message = localize('profileNameRequired', "Profile name is required.");
			return;
		}
		if (this.name !== this.getInitialName() && this.userDataProfilesService.profiles.some(p => p.name === this.name)) {
			this.message = localize('profileExists', "Profile with name {0} already exists.", this.name);
			return;
		}
		if (
			this.flags && this.flags.settings && this.flags.keybindings && this.flags.tasks && this.flags.snippets && this.flags.extensions
		) {
			this.message = localize('invalid configurations', "The profile should contain at least one configuration.");
			return;
		}
		this.message = undefined;
	}

	async getChildren(resourceType?: ProfileResourceType): Promise<IProfileChildElement[]> {
		if (resourceType === undefined) {
			const resourceTypes = [
				ProfileResourceType.Settings,
				ProfileResourceType.Keybindings,
				ProfileResourceType.Tasks,
				ProfileResourceType.Snippets,
				ProfileResourceType.Extensions
			];
			return resourceTypes.map<IProfileResourceTypeElement>(resourceType => ({
				handle: resourceType,
				checkbox: undefined,
				resourceType,
				action: resourceType === ProfileResourceType.Settings
					|| resourceType === ProfileResourceType.Keybindings
					|| resourceType === ProfileResourceType.Tasks
					? new Action('_open', '', undefined, true, async () => {
						const children = await this.getChildren(resourceType);
						children[0]?.action?.run();
					}) : undefined
			}));
		}
		return this.getChildrenForResourceType(resourceType);
	}

	protected async getChildrenForResourceType(resourceType: ProfileResourceType): Promise<IProfileChildElement[]> {
		return [];
	}

	protected async getChildrenFromProfile(profile: IUserDataProfile, resourceType: ProfileResourceType): Promise<IProfileResourceTypeChildElement[]> {
		profile = this.getFlag(resourceType) ? this.userDataProfilesService.defaultProfile : profile;
		let children: IProfileResourceChildTreeItem[] = [];
		switch (resourceType) {
			case ProfileResourceType.Settings:
				children = await this.instantiationService.createInstance(SettingsResourceTreeItem, profile).getChildren();
				break;
			case ProfileResourceType.Keybindings:
				children = await this.instantiationService.createInstance(KeybindingsResourceTreeItem, profile).getChildren();
				break;
			case ProfileResourceType.Snippets:
				children = (await this.instantiationService.createInstance(SnippetsResourceTreeItem, profile).getChildren()) ?? [];
				break;
			case ProfileResourceType.Tasks:
				children = await this.instantiationService.createInstance(TasksResourceTreeItem, profile).getChildren();
				break;
			case ProfileResourceType.Extensions:
				children = await this.instantiationService.createInstance(ExtensionsResourceExportTreeItem, profile).getChildren();
				break;
		}
		return children.map<IProfileResourceTypeChildElement>(child => this.toUserDataProfileResourceChildElement(child));
	}

	protected toUserDataProfileResourceChildElement(child: IProfileResourceChildTreeItem): IProfileResourceTypeChildElement {
		return {
			handle: child.handle,
			checkbox: child.checkbox,
			label: child.label?.label ?? '',
			resource: URI.revive(child.resourceUri),
			icon: child.themeIcon,
			action: new Action('_openChild', '', undefined, true, async () => {
				if (child.parent.type === ProfileResourceType.Extensions) {
					await this.commandService.executeCommand('extension.open', child.handle, undefined, true, undefined, true);
				} else if (child.resourceUri) {
					await this.commandService.executeCommand(API_OPEN_EDITOR_COMMAND_ID, child.resourceUri, [SIDE_GROUP], undefined);
				}
			})
		};

	}

	protected getInitialName(): string {
		return '';
	}

	save(): void {
		this.saveScheduler.schedule();
	}

	private hasUnsavedChanges(profile: IUserDataProfile): boolean {
		if (this.name !== profile.name) {
			return true;
		}
		if (this.icon !== profile.icon) {
			return true;
		}
		if (!equals(this.flags ?? {}, profile.useDefaultFlags ?? {})) {
			return true;
		}
		return false;
	}

	protected async saveProfile(profile: IUserDataProfile): Promise<IUserDataProfile | undefined> {
		if (!this.hasUnsavedChanges(profile)) {
			return;
		}
		this.validate();
		if (this.message) {
			return;
		}
		const useDefaultFlags: UseDefaultProfileFlags | undefined = this.flags
			? this.flags.settings && this.flags.keybindings && this.flags.tasks && this.flags.globalState && this.flags.extensions ? undefined : this.flags
			: undefined;

		return await this.userDataProfileManagementService.updateProfile(profile, {
			name: this.name,
			icon: this.icon,
			useDefaultFlags: profile.useDefaultFlags && !useDefaultFlags ? {} : useDefaultFlags
		});
	}

	abstract readonly primaryAction?: Action;
	abstract readonly secondaryAction?: Action;
	abstract readonly titleActions: [IAction[], IAction[]];
	abstract readonly contextMenuActions: IAction[];

	protected abstract doSave(): Promise<void>;
}

export class UserDataProfileElement extends AbstractUserDataProfileElement {

	get profile(): IUserDataProfile { return this._profile; }

	readonly primaryAction = undefined;
	readonly secondaryAction = undefined;

	constructor(
		private _profile: IUserDataProfile,
		readonly titleActions: [IAction[], IAction[]],
		readonly contextMenuActions: IAction[],
		@IUserDataProfileService private readonly userDataProfileService: IUserDataProfileService,
		@IConfigurationService private readonly configurationService: IConfigurationService,
		@IUserDataProfileManagementService userDataProfileManagementService: IUserDataProfileManagementService,
		@IUserDataProfilesService userDataProfilesService: IUserDataProfilesService,
		@ICommandService commandService: ICommandService,
		@IInstantiationService instantiationService: IInstantiationService,
	) {
		super(
			_profile.name,
			_profile.icon,
			_profile.useDefaultFlags,
			userDataProfileService.currentProfile.id === _profile.id,
			userDataProfileManagementService,
			userDataProfilesService,
			commandService,
			instantiationService,
		);
		this._isNewWindowProfile = this.configurationService.getValue(CONFIG_NEW_WINDOW_PROFILE) === this.profile.name;
		this._register(configurationService.onDidChangeConfiguration(e => {
			if (e.affectsConfiguration(CONFIG_NEW_WINDOW_PROFILE)) {
				this.isNewWindowProfile = this.configurationService.getValue(CONFIG_NEW_WINDOW_PROFILE) === this.profile.name;
			}
		}
		));
		this._register(this.userDataProfileService.onDidChangeCurrentProfile(() => this.active = this.userDataProfileService.currentProfile.id === this.profile.id));
		this._register(this.userDataProfilesService.onDidChangeProfiles(() => {
			const profile = this.userDataProfilesService.profiles.find(p => p.id === this.profile.id);
			if (profile) {
				this._profile = profile;
				this.name = profile.name;
				this.icon = profile.icon;
				this.flags = profile.useDefaultFlags;
			}
		}));
	}

	public async toggleNewWindowProfile(): Promise<void> {
		if (this._isNewWindowProfile) {
			await this.configurationService.updateValue(CONFIG_NEW_WINDOW_PROFILE, null);
		} else {
			await this.configurationService.updateValue(CONFIG_NEW_WINDOW_PROFILE, this.profile.name);
		}
	}

	private _isNewWindowProfile: boolean = false;
	get isNewWindowProfile(): boolean { return this._isNewWindowProfile; }
	set isNewWindowProfile(isNewWindowProfile: boolean) {
		if (this._isNewWindowProfile !== isNewWindowProfile) {
			this._isNewWindowProfile = isNewWindowProfile;
			this._onDidChange.fire({ newWindowProfile: true });
		}
	}

	protected override async doSave(): Promise<void> {
		await this.saveProfile(this.profile);
	}

	protected override async getChildrenForResourceType(resourceType: ProfileResourceType): Promise<IProfileChildElement[]> {
		return this.getChildrenFromProfile(this.profile, resourceType);
	}

	protected override getInitialName(): string {
		return this.profile.name;
	}

}

const USER_DATA_PROFILE_TEMPLATE_PREVIEW_SCHEME = 'userdataprofiletemplatepreview';

export class NewProfileElement extends AbstractUserDataProfileElement {

	constructor(
		name: string,
		copyFrom: URI | IUserDataProfile | undefined,
		readonly primaryAction: Action,
		readonly secondaryAction: Action,
		readonly titleActions: [IAction[], IAction[]],
		readonly contextMenuActions: Action[],
		@IFileService private readonly fileService: IFileService,
		@IUserDataProfileImportExportService private readonly userDataProfileImportExportService: IUserDataProfileImportExportService,
		@IUserDataProfileManagementService userDataProfileManagementService: IUserDataProfileManagementService,
		@IUserDataProfilesService userDataProfilesService: IUserDataProfilesService,
		@ICommandService commandService: ICommandService,
		@IInstantiationService instantiationService: IInstantiationService,
	) {
		super(
			name,
			undefined,
			undefined,
			false,
			userDataProfileManagementService,
			userDataProfilesService,
			commandService,
			instantiationService,
		);
		this._copyFrom = copyFrom;
		this._copyFlags = this.getCopyFlagsFrom(copyFrom);
		this._register(this.fileService.registerProvider(USER_DATA_PROFILE_TEMPLATE_PREVIEW_SCHEME, this._register(new InMemoryFileSystemProvider())));
	}

	private _copyFrom: IUserDataProfile | URI | undefined;
	get copyFrom(): IUserDataProfile | URI | undefined { return this._copyFrom; }
	set copyFrom(copyFrom: IUserDataProfile | URI | undefined) {
		if (this._copyFrom !== copyFrom) {
			this._copyFrom = copyFrom;
			this._onDidChange.fire({ copyFrom: true });
			this.flags = undefined;
			this.copyFlags = this.getCopyFlagsFrom(copyFrom);
		}
	}

	private _copyFlags: ProfileResourceTypeFlags | undefined;
	get copyFlags(): ProfileResourceTypeFlags | undefined { return this._copyFlags; }
	set copyFlags(flags: ProfileResourceTypeFlags | undefined) {
		if (!equals(this._copyFlags, flags)) {
			this._copyFlags = flags;
			this._onDidChange.fire({ copyFlags: true });
		}
	}

	private _previewProfile: IUserDataProfile | undefined;
	get previewProfile(): IUserDataProfile | undefined { return this._previewProfile; }
	set previewProfile(profile: IUserDataProfile | undefined) {
		if (this._previewProfile !== profile) {
			this._previewProfile = profile;
			this._onDidChange.fire({ preview: true });
		}
	}

	private getCopyFlagsFrom(copyFrom: URI | IUserDataProfile | undefined): ProfileResourceTypeFlags | undefined {
		return copyFrom ? {
			settings: true,
			keybindings: true,
			snippets: true,
			tasks: true,
			extensions: true
		} : undefined;
	}

	getCopyFlag(key: ProfileResourceType): boolean {
		return this.copyFlags?.[key] ?? false;
	}

	setCopyFlag(key: ProfileResourceType, value: boolean): void {
		const flags = this.copyFlags ? { ...this.copyFlags } : {};
		flags[key] = value;
		this.copyFlags = flags;
	}

	protected override async getChildrenForResourceType(resourceType: ProfileResourceType): Promise<IProfileChildElement[]> {
		if (this.getFlag(resourceType)) {
			return this.getChildrenFromProfile(this.userDataProfilesService.defaultProfile, resourceType);
		}
		if (!this.getCopyFlag(resourceType)) {
			return [];
		}
		if (this.copyFrom instanceof URI) {
			const template = await this.userDataProfileImportExportService.resolveProfileTemplate(this.copyFrom);
			if (!template) {
				return [];
			}
			return this.getChildrenFromProfileTemplate(template, resourceType);
		}
		if (this.copyFrom) {
			return this.getChildrenFromProfile(this.copyFrom, resourceType);
		}
		return [];
	}

	private async getChildrenFromProfileTemplate(profileTemplate: IUserDataProfileTemplate, resourceType: ProfileResourceType): Promise<IProfileResourceTypeChildElement[]> {
		const profile = toUserDataProfile(generateUuid(), this.name, URI.file('/root').with({ scheme: USER_DATA_PROFILE_TEMPLATE_PREVIEW_SCHEME }), URI.file('/cache').with({ scheme: USER_DATA_PROFILE_TEMPLATE_PREVIEW_SCHEME }));
		switch (resourceType) {
			case ProfileResourceType.Settings:
				if (profileTemplate.settings) {
					await this.instantiationService.createInstance(SettingsResource).apply(profileTemplate.settings, profile);
					return this.getChildrenFromProfile(profile, resourceType);
				}
				return [];
			case ProfileResourceType.Keybindings:
				if (profileTemplate.keybindings) {
					await this.instantiationService.createInstance(KeybindingsResource).apply(profileTemplate.keybindings, profile);
					return this.getChildrenFromProfile(profile, resourceType);
				}
				return [];
			case ProfileResourceType.Snippets:
				if (profileTemplate.snippets) {
					await this.instantiationService.createInstance(SnippetsResource).apply(profileTemplate.snippets, profile);
					return this.getChildrenFromProfile(profile, resourceType);
				}
				return [];
			case ProfileResourceType.Tasks:
				if (profileTemplate.tasks) {
					await this.instantiationService.createInstance(TasksResource).apply(profileTemplate.tasks, profile);
					return this.getChildrenFromProfile(profile, resourceType);
				}
				return [];
			case ProfileResourceType.Extensions:
				if (profileTemplate.extensions) {
					const children = await this.instantiationService.createInstance(ExtensionsResourceImportTreeItem, profileTemplate.extensions).getChildren();
					return children.map(child => this.toUserDataProfileResourceChildElement(child));
				}
				return [];
		}
		return [];
	}

	protected override getInitialName(): string {
		return this.previewProfile?.name ?? '';
	}

	protected override async doSave(): Promise<void> {
		if (this.previewProfile) {
			const profile = await this.saveProfile(this.previewProfile);
			if (profile) {
				this.previewProfile = profile;
			}
		}
	}
}

export class UserDataProfilesEditorModel extends EditorModel {

	private static INSTANCE: UserDataProfilesEditorModel | undefined;
	static getInstance(instantiationService: IInstantiationService): UserDataProfilesEditorModel {
		if (!UserDataProfilesEditorModel.INSTANCE) {
			UserDataProfilesEditorModel.INSTANCE = instantiationService.createInstance(UserDataProfilesEditorModel);
		}
		return UserDataProfilesEditorModel.INSTANCE;
	}

	private _profiles: [AbstractUserDataProfileElement, DisposableStore][] = [];
	get profiles(): AbstractUserDataProfileElement[] {
		return this._profiles
			.map(([profile]) => profile)
			.sort((a, b) => {
				if (a instanceof NewProfileElement) {
					return 1;
				}
				if (b instanceof NewProfileElement) {
					return -1;
				}
				if (a instanceof UserDataProfileElement && a.profile.isDefault) {
					return -1;
				}
				if (b instanceof UserDataProfileElement && b.profile.isDefault) {
					return 1;
				}
				return a.name.localeCompare(b.name);
			});
	}

	private newProfileElement: NewProfileElement | undefined;

	private _onDidChange = this._register(new Emitter<AbstractUserDataProfileElement | undefined>());
	readonly onDidChange = this._onDidChange.event;

	constructor(
		@IUserDataProfileService private readonly userDataProfileService: IUserDataProfileService,
		@IUserDataProfilesService private readonly userDataProfilesService: IUserDataProfilesService,
		@IUserDataProfileManagementService private readonly userDataProfileManagementService: IUserDataProfileManagementService,
		@IUserDataProfileImportExportService private readonly userDataProfileImportExportService: IUserDataProfileImportExportService,
		@IDialogService private readonly dialogService: IDialogService,
		@ITelemetryService private readonly telemetryService: ITelemetryService,
		@IHostService private readonly hostService: IHostService,
		@IInstantiationService private readonly instantiationService: IInstantiationService,
	) {
		super();
		for (const profile of userDataProfilesService.profiles) {
			if (!profile.isTransient) {
				this._profiles.push(this.createProfileElement(profile));
			}
		}
		this._register(toDisposable(() => this._profiles.splice(0, this._profiles.length).map(([, disposables]) => disposables.dispose())));
		this._register(userDataProfilesService.onDidChangeProfiles(e => this.onDidChangeProfiles(e)));
	}

	private onDidChangeProfiles(e: DidChangeProfilesEvent): void {
		for (const profile of e.added) {
			if (!profile.isTransient && profile.name !== this.newProfileElement?.name) {
				this._profiles.push(this.createProfileElement(profile));
			}
		}
		for (const profile of e.removed) {
			if (profile.id === this.newProfileElement?.previewProfile?.id) {
				this.newProfileElement.previewProfile = undefined;
			}
			const index = this._profiles.findIndex(([p]) => p instanceof UserDataProfileElement && p.profile.id === profile.id);
			if (index !== -1) {
				this._profiles.splice(index, 1).map(([, disposables]) => disposables.dispose());
			}
		}
		this._onDidChange.fire(undefined);
	}

	private createProfileElement(profile: IUserDataProfile): [UserDataProfileElement, DisposableStore] {
		const disposables = new DisposableStore();

		const activateAction = disposables.add(new Action('userDataProfile.activate', localize('active', "Active"), ThemeIcon.asClassName(Codicon.check), true, () => this.userDataProfileManagementService.switchProfile(profile)));
		activateAction.checked = this.userDataProfileService.currentProfile.id === profile.id;
		disposables.add(this.userDataProfileService.onDidChangeCurrentProfile(() => activateAction.checked = this.userDataProfileService.currentProfile.id === profile.id));
		const copyFromProfileAction = disposables.add(new Action('userDataProfile.copyFromProfile', localize('copyFromProfile', "Save As..."), ThemeIcon.asClassName(Codicon.copy), true, () => this.createNewProfile(profile)));
		const exportAction = disposables.add(new Action('userDataProfile.export', localize('export', "Export..."), ThemeIcon.asClassName(Codicon.export), true, () => this.exportProfile(profile)));
		const deleteAction = disposables.add(new Action('userDataProfile.delete', localize('delete', "Delete"), ThemeIcon.asClassName(Codicon.trash), true, () => this.removeProfile(profile)));
		const newWindowAction = disposables.add(new Action('userDataProfile.newWindow', localize('new window', "New {0} Window", profile.name), ThemeIcon.asClassName(Codicon.emptyWindow), true, () => this.openWindow(profile)));
		const useAsNewWindowProfileAction = disposables.add(new Action('userDataProfile.useAsNewWindowProfile', localize('use as new window', "Enable for New Windows", profile.name), undefined, true, () => profileElement.toggleNewWindowProfile()));

		const titlePrimaryActions: IAction[] = [];
		titlePrimaryActions.push(copyFromProfileAction);
		titlePrimaryActions.push(exportAction);
		if (!profile.isDefault) {
			titlePrimaryActions.push(deleteAction);
		}

		const titleSecondaryActions: IAction[] = [];

		const secondaryActions: IAction[] = [];
		secondaryActions.push(activateAction);
		secondaryActions.push(useAsNewWindowProfileAction);
		secondaryActions.push(new Separator());
		secondaryActions.push(newWindowAction);
		secondaryActions.push(new Separator());
		secondaryActions.push(copyFromProfileAction);
		secondaryActions.push(exportAction);
		if (!profile.isDefault) {
			secondaryActions.push(new Separator());
			secondaryActions.push(deleteAction);
		}
		const profileElement = disposables.add(this.instantiationService.createInstance(UserDataProfileElement,
			profile,
			[titlePrimaryActions, titleSecondaryActions],
			secondaryActions,
		));

		useAsNewWindowProfileAction.checked = profileElement.isNewWindowProfile;
		useAsNewWindowProfileAction.label = profileElement.isNewWindowProfile ? localize('donot use as new window', "Disable for New Windows") : localize('use as new window', "Enable for New Windows");

		disposables.add(profileElement.onDidChange(e => {
			if (e.name) {
				newWindowAction.label = localize('new window', "New {0} Window", profileElement.name);
			}
			if (e.newWindowProfile) {
				useAsNewWindowProfileAction.checked = profileElement.isNewWindowProfile;
				useAsNewWindowProfileAction.label = profileElement.isNewWindowProfile ? localize('donot use as new window', "Disable for New Windows") : localize('use as new window', "Enable for New Windows");
			}
		}));
		return [profileElement, disposables];
	}

	createNewProfile(copyFrom?: URI | IUserDataProfile): AbstractUserDataProfileElement {
		if (!this.newProfileElement) {
			const disposables = new DisposableStore();
			const cancellationTokenSource = new CancellationTokenSource();
			disposables.add(toDisposable(() => cancellationTokenSource.dispose(true)));
			const createAction = disposables.add(new Action(
				'userDataProfile.create',
				localize('create', "Create"),
				undefined,
				true,
				() => this.saveNewProfile(false, cancellationTokenSource.token)
			));
			const cancelAction = disposables.add(new Action(
				'userDataProfile.cancel',
				localize('cancel', "Cancel"),
				ThemeIcon.asClassName(Codicon.close),
				true,
				() => this.discardNewProfile()
			));
			const previewProfileAction = disposables.add(new Action(
				'userDataProfile.preview',
				localize('preview', "Open Preview"),
				ThemeIcon.asClassName(Codicon.openPreview),
				true,
				() => this.previewNewProfile(cancellationTokenSource.token)
			));
			this.newProfileElement = disposables.add(this.instantiationService.createInstance(NewProfileElement,
				localize('untitled', "Untitled"),
				copyFrom,
				createAction,
				cancelAction,
				[[
					previewProfileAction
				], []],
				[],
			));
			disposables.add(this.newProfileElement.onDidChange(e => {
				if (e.preview) {
					previewProfileAction.checked = !!this.newProfileElement?.previewProfile;
				}
				if (e.disabled) {
					previewProfileAction.enabled = !this.newProfileElement?.disabled;
					createAction.enabled = !this.newProfileElement?.disabled;
				}
			}));
			this._profiles.push([this.newProfileElement, disposables]);
			this._onDidChange.fire(this.newProfileElement);
		}
		return this.newProfileElement;
	}

	revert(): void {
		this.removeNewProfile();
		this._onDidChange.fire(undefined);
	}

	private removeNewProfile(): void {
		if (this.newProfileElement) {
			const index = this._profiles.findIndex(([p]) => p === this.newProfileElement);
			if (index !== -1) {
				this._profiles.splice(index, 1).map(([, disposables]) => disposables.dispose());
			}
			this.newProfileElement = undefined;
		}
	}

	private async previewNewProfile(token: CancellationToken): Promise<void> {
		if (!this.newProfileElement) {
			return;
		}
		if (this.newProfileElement.previewProfile) {
			return;
		}
		const profile = await this.saveNewProfile(true, token);
		if (profile) {
			this.newProfileElement.previewProfile = profile;
			await this.openWindow(profile);
		}
	}

	async saveNewProfile(transient?: boolean, token?: CancellationToken): Promise<IUserDataProfile | undefined> {
		if (!this.newProfileElement) {
			return undefined;
		}

		this.newProfileElement.validate();
		if (this.newProfileElement.message) {
			return undefined;
		}

		this.newProfileElement.disabled = true;
		let profile: IUserDataProfile | undefined;

		try {
			if (this.newProfileElement.previewProfile) {
				if (!transient) {
					profile = await this.userDataProfileManagementService.updateProfile(this.newProfileElement.previewProfile, { transient: false });
				}
			}
			else {
				const { flags, icon, name, copyFrom } = this.newProfileElement;
				const useDefaultFlags: UseDefaultProfileFlags | undefined = flags
					? flags.settings && flags.keybindings && flags.tasks && flags.globalState && flags.extensions ? undefined : flags
					: undefined;

				type CreateProfileInfoClassification = {
					owner: 'sandy081';
					comment: 'Report when profile is about to be created';
					source: { classification: 'SystemMetaData'; purpose: 'FeatureInsight'; comment: 'Type of profile source' };
				};
				type CreateProfileInfoEvent = {
					source: string | undefined;
				};
				const createProfileTelemetryData: CreateProfileInfoEvent = { source: copyFrom instanceof URI ? 'template' : isUserDataProfile(copyFrom) ? 'profile' : copyFrom ? 'external' : undefined };

				if (copyFrom instanceof URI) {
					this.telemetryService.publicLog2<CreateProfileInfoEvent, CreateProfileInfoClassification>('userDataProfile.createFromTemplate', createProfileTelemetryData);
					await this.userDataProfileImportExportService.importProfile(copyFrom, { mode: 'apply', name: name, useDefaultFlags, icon: icon ? icon : undefined, resourceTypeFlags: this.newProfileElement.copyFlags, donotSwitch: true, transient });
				} else if (isUserDataProfile(copyFrom)) {
					this.telemetryService.publicLog2<CreateProfileInfoEvent, CreateProfileInfoClassification>('userDataProfile.createFromProfile', createProfileTelemetryData);
					await this.userDataProfileImportExportService.createFromProfile(copyFrom, name, { useDefaultFlags, icon: icon ? icon : undefined, resourceTypeFlags: this.newProfileElement.copyFlags, donotSwitch: true, transient });
				} else {
					this.telemetryService.publicLog2<CreateProfileInfoEvent, CreateProfileInfoClassification>('userDataProfile.createEmptyProfile', createProfileTelemetryData);
					await this.userDataProfileManagementService.createProfile(name, { useDefaultFlags, icon: icon ? icon : undefined, transient });
				}

				profile = this.userDataProfilesService.profiles.find(p => p.name === name);
			}
		} finally {
			if (this.newProfileElement) {
				this.newProfileElement.disabled = false;
			}
		}

		if (token?.isCancellationRequested) {
			if (profile) {
				try {
					await this.userDataProfileManagementService.removeProfile(profile);
				} catch (error) {
					// ignore
				}
			}
			return;
		}

		if (profile && !profile.isTransient && this.newProfileElement) {
			this.removeNewProfile();
			this.onDidChangeProfiles({ added: [profile], removed: [], updated: [], all: this.userDataProfilesService.profiles });
		}

		return profile;
	}

	private async discardNewProfile(): Promise<void> {
		if (!this.newProfileElement) {
			return;
		}
		if (this.newProfileElement.previewProfile) {
			await this.userDataProfileManagementService.removeProfile(this.newProfileElement.previewProfile);
		}
		this.removeNewProfile();
		this._onDidChange.fire(undefined);
	}

	private async removeProfile(profile: IUserDataProfile): Promise<void> {
		const result = await this.dialogService.confirm({
			type: 'info',
			message: localize('deleteProfile', "Are you sure you want to delete the profile '{0}'?", profile.name),
			primaryButton: localize('delete', "Delete"),
			cancelButton: localize('cancel', "Cancel")
		});
		if (result.confirmed) {
			await this.userDataProfileManagementService.removeProfile(profile);
		}
	}

	private async openWindow(profile: IUserDataProfile): Promise<void> {
		await this.hostService.openWindow({ forceProfile: profile.name });
	}

	private async exportProfile(profile: IUserDataProfile): Promise<void> {
		return this.userDataProfileImportExportService.exportProfile2(profile);
	}
}
