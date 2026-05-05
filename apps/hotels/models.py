from django.db import models
from apps.core.models import TimeStampedModel
class Hotel(TimeStampedModel):
    name=models.CharField(max_length=120)
    code=models.SlugField(unique=True)
