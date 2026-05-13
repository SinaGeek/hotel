from django.contrib.auth.models import AbstractUser
from django.db import models
from apps.core.models import TimeStampedModel, SoftDeleteModel
from django.utils.translation import gettext_lazy as _

class PermissionDefinition(TimeStampedModel):
    codename = models.CharField(max_length=100, unique=True, verbose_name=_("Codename"))
    name = models.CharField(max_length=200, verbose_name=_("Display Name"))

    def __str__(self):
        return self.name

    class Meta:
        verbose_name = _("Permission Definition")
        verbose_name_plural = _("Permission Definitions")

class Role(TimeStampedModel):
    name = models.CharField(max_length=50, unique=True, verbose_name=_("Role Name"))
    description = models.TextField(blank=True, verbose_name=_("Duty Description"))
    permissions_list = models.ManyToManyField(PermissionDefinition, blank=True, verbose_name=_("Access List"))
    
    # Keeping the old field for backward compatibility or migration if needed, 
    # but we will primarily use permissions_list for the UI.
    permissions = models.JSONField(default=list, verbose_name=_("Access (JSON)"), blank=True)

    def save(self, *args, **kwargs):
        super().save(*args, **kwargs)

    def __str__(self):
        return self.name

    @property
    def is_admin(self):
        return self.name.lower() == 'admin'

    @property
    def is_reception(self):
        name = self.name.lower()
        return 'reception' in name or 'recep' in name

    @property
    def is_financial(self):
        name = self.name.lower()
        return any(x in name for x in ['financial', 'accountant', 'accountants', 'finance'])

    class Meta:
        verbose_name = _("Role")
        verbose_name_plural = _("Roles")

class User(AbstractUser, SoftDeleteModel):
    hotel_id = models.BigIntegerField(db_index=True, null=True, blank=True, verbose_name=_("Hotel ID"))
    role = models.ForeignKey(Role, null=True, blank=True, on_delete=models.SET_NULL, verbose_name=_("Role"))
    avatar = models.ImageField(upload_to='accounts/avatars/', null=True, blank=True, verbose_name=_("Avatar"))

    def has_custom_perm(self, perm_name):
        if self.is_superuser:
            return True
        if not self.role:
            return False
        # Check in the new many-to-many relationship
        return self.role.permissions_list.filter(codename=perm_name).exists()

    @property
    def is_admin(self):
        return self.is_superuser or (self.role and self.role.is_admin)

    @property
    def is_reception(self):
        return self.role and self.role.is_reception

    @property
    def is_financial(self):
        return self.role and self.role.is_financial

    class Meta:
        verbose_name = _("User")
        verbose_name_plural = _("Users")

class ActivityLog(models.Model):
    user = models.ForeignKey(User, on_delete=models.CASCADE, related_name='activity_logs', verbose_name=_("User"))
    action = models.TextField(verbose_name=_("Operation"))
    timestamp = models.DateTimeField(auto_now_add=True, verbose_name=_("Timestamp"))
    is_visible_to_user = models.BooleanField(default=False, verbose_name=_("Visible to User"))

    class Meta:
        verbose_name = _("Activity Log")
        verbose_name_plural = _("Activity Logs")
        ordering = ['-timestamp']