from django.contrib.auth.models import AbstractUser
from django.db import models
from apps.core.models import TimeStampedModel, SoftDeleteModel

class Role(TimeStampedModel):
    name=models.CharField(max_length=50, unique=True)
    permissions=models.JSONField(default=list)

class User(AbstractUser, SoftDeleteModel):
    hotel_id=models.BigIntegerField(db_index=True, null=True, blank=True)
    role=models.ForeignKey(Role, null=True, blank=True, on_delete=models.SET_NULL)
